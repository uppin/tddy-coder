//! Where a root's index stands, as the root's own server reports it: whether its crate graph is
//! loaded, whether the server is busy right now, and every phase it has in flight.
//!
//! `Warm` used to answer "yes" the moment [`tddy_lsp::LspRegistry`] handed back a live server,
//! which is a weaker claim than the schema makes: measured on a real three-crate workspace, a warm
//! returned in 0.34s while the first `Anchors` on the same root — which does wait for the graph —
//! took 2.1s. So `ready` could be true while the next request still paid the whole load.
//!
//! rust-analyzer says when its graph is queryable: `experimental/serverStatus` with
//! `quiescent: true`, on the transition and **only** on the transition. That is the whole reason
//! this is a per-root latch fed by a watcher attached at spawn rather than something each request
//! reads for itself: a request arriving after the transition would wait for ever to be told
//! something the server has already said once, to somebody else.
//!
//! The watcher folds through [`ServerChatter`] rather than reading `quiescent` out of the JSON
//! itself, because there is one right way to read these two notifications and two readings of it
//! would drift.
//!
//! **The fold runs for the server's whole life, not only until the graph loads.** It is the root's
//! progress publisher: anyone — a warm, a `WatchRoot`, a Code pane joining mid-load — reads the
//! latest [`RootProgress`] from it, so a watcher that arrives late still sees every phase already
//! running rather than reports whose `begin` it never saw. The loaded latch stays what it was: the
//! first quiescence, never unset.

use tddy_code_restructuring::backends::rust::{ActivePhase, ServerChatter};
use tddy_lsp::client::LspClient;
use tddy_lsp::{NotificationEvent, NotificationStream};
use tokio::sync::watch;

/// One root's progress as its server has reported it, folded from every notification since spawn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RootProgress {
    /// Whether the server's **latest** `experimental/serverStatus` said it is quiescent. Not a
    /// latch: an edit that sends the server back to work makes this false again, which is what a
    /// display and a held query need to know. False before the server has said anything.
    pub(crate) quiescent: bool,
    /// Whether the crate graph has been observed loaded — the first quiescence, never unset.
    pub(crate) loaded: bool,
    /// Every work-done progress token in flight, in the order the server began them.
    pub(crate) phases: Vec<ActivePhase>,
    /// The phase the server reported on most recently, and the highest percentage it reached.
    pub(crate) current: Option<(u64, String)>,
    /// How far the load got: the last line with the furthest percentage any phase reached.
    pub(crate) furthest: String,
    /// The latest line worth showing; empty until the server has reported any progress.
    pub(crate) line: String,
}

impl RootProgress {
    /// This progress with `chatter`'s latest reading folded in, `line` replacing the last line
    /// when there is one. Returns whether anything a watcher reads changed.
    fn fold(&mut self, chatter: &ServerChatter, line: Option<String>) -> bool {
        let next = RootProgress {
            quiescent: chatter.quiescent(),
            loaded: self.loaded || chatter.quiescent(),
            phases: chatter.active_phases(),
            current: chatter
                .current()
                .map(|(percentage, phase)| (percentage, phase.to_string())),
            furthest: chatter.how_far(),
            line: line.unwrap_or_else(|| self.line.clone()),
        };
        let changed = next != *self;
        *self = next;
        changed
    }
}

/// One root's progress, published by the watcher attached to its server.
///
/// The loaded latch travels inside it ([`RootProgress::loaded`]), so a follower reads "is the graph
/// loaded?" and "what is the server doing now?" from one value and cannot see them disagree.
///
/// Cloneable, and every clone reads the same publisher: one watcher per server, any number of
/// requests and watches following it.
#[derive(Clone)]
pub(crate) struct GraphLoad {
    progress: watch::Receiver<RootProgress>,
}

impl GraphLoad {
    /// Attach a watcher to `client` and return the progress it publishes.
    ///
    /// Called once per spawned server, from the host that spawned it — which is the only moment
    /// early enough to be sure of catching the transition, and the `begin` of every phase. The
    /// watcher outlives every request: it ends when the client it watches is dropped, which is what
    /// makes a dropped sender mean "the server this was about is gone" rather than "nothing
    /// happened yet".
    pub(crate) fn watching(client: &LspClient) -> Self {
        let (published, progress) = watch::channel(RootProgress::default());
        let notifications = client.subscribe_notifications();
        tokio::spawn(fold_the_servers_progress(notifications, published));
        Self { progress }
    }

    /// The root's progress, to follow change by change. Its sender is dropped when the server it
    /// is about is gone.
    pub(crate) fn progress(&self) -> watch::Receiver<RootProgress> {
        self.progress.clone()
    }

    /// Whether the watcher feeding this progress is still attached.
    ///
    /// False once the server it watched is gone, which is what tells the host to attach a fresh
    /// watcher to a respawned server rather than reading progress about a dead one.
    pub(crate) fn still_watching(&self) -> bool {
        self.progress.has_changed().is_ok()
    }
}

/// Fold `notifications` into the root's published progress for as long as the server lives.
///
/// The loaded latch is set on the first quiescence and never unset: a graph that has loaded does
/// not unload, and a server going non-quiescent to answer an edit is not a root that has to be
/// warmed again. Everything else follows the server — its latest status and the phases it has in
/// flight — because that is what a watcher joining at any moment needs to read.
///
/// The task does not end until the client does, because dropping the sender is how this crate says
/// "the server this was about is gone". A watcher that returned early would make every later reader
/// see a dead server and attach a fresh watcher to a live one, which would then wait for a
/// transition already made.
///
/// A watcher that falls behind is told how many notifications it lost, and it cannot know whether
/// the transition, or a phase's `end`, was among them. It therefore keeps reading rather than
/// latching — claiming a load on the strength of a gap is exactly the silent guess this latch
/// exists instead of — and says so in the published line, with the furthest the load got.
async fn fold_the_servers_progress(
    mut notifications: NotificationStream,
    published: watch::Sender<RootProgress>,
) {
    let mut chatter = ServerChatter::unthrottled();
    loop {
        let line = match notifications.recv().await {
            NotificationEvent::Received(notification) => chatter.absorb(&notification),
            NotificationEvent::Lost(lost) => {
                log::debug!(
                    target: "tddy_index_daemon::graph",
                    "lost sight of {lost} notification(s) while following a root's progress"
                );
                Some(lost_sight_of(lost, &chatter))
            }
            // The client is gone, so nothing will ever report on this server again. Dropping the
            // sender is the signal: every follower learns the server went away instead of waiting
            // on a transition that can no longer happen.
            NotificationEvent::Ended => return,
        };
        published.send_if_modified(|progress| progress.fold(&chatter, line));
    }
}

/// What the progress says when its watcher fell behind the server it is watching.
///
/// Reported as a line of its own, with the furthest the load had got attached, and the fold carries
/// on: the graph is no less likely to be loading for the gap. Silence here is the failure this
/// account exists to replace — a follower that lost the progress it was reading and said nothing
/// leaves its reader unable to tell a load from a hang.
fn lost_sight_of(lost: u64, chatter: &ServerChatter) -> String {
    format!(
        "lost sight of {lost} progress notification(s) the server sent faster than this wait read \
         them; furthest so far: {}",
        chatter.how_far()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// What a watcher of the root is sent once `chatter`'s reading and `line` are folded in.
    fn reported_after(
        chatter: &ServerChatter,
        line: String,
    ) -> crate::proto::code_index::IndexProgress {
        let mut progress = RootProgress::default();
        progress.fold(chatter, Some(line));
        crate::watch_root::reported(&progress)
    }

    /// A phase's percentage must be the furthest the load reached, not the last number to arrive.
    /// The server counts files inside a phase and then emits sub-steps with no percentage at all,
    /// so the last notification routinely has no number in it — and a `percentage` that went
    /// backwards would read as a load going wrong rather than one still going.
    #[test]
    fn carries_the_furthest_the_load_reached_rather_than_the_last_number_to_arrive() {
        // Given a server that reported 64% and then a sub-step carrying no percentage
        let mut chatter = ServerChatter::default();
        let started = std::time::Instant::now();
        let after = |seconds| started + std::time::Duration::from_secs(seconds);
        chatter.absorb_at(
            &json!({
                "method": "$/progress",
                "params": {
                    "token": "load",
                    "value": { "kind": "begin", "title": "loading crate graph" },
                }
            }),
            started,
        );
        chatter.absorb_at(
            &json!({
                "method": "$/progress",
                "params": { "token": "load", "value": { "kind": "report", "percentage": 64 } }
            }),
            after(1),
        );
        let line = chatter
            .absorb_at(
                &json!({
                    "method": "$/progress",
                    "params": {
                        "token": "load",
                        "value": { "kind": "report", "message": "tddy_desktop (lib)" },
                    }
                }),
                after(3),
            )
            .expect("a sub-step, once the interval has passed, is worth a line");

        // When that line becomes a progress message
        let progress = reported_after(&chatter, line);

        // Then the phase and the percentage are the furthest the load got, and the line is the
        // sub-step that arrived
        assert_eq!(
            (
                progress.line.as_str(),
                progress.phase.as_str(),
                progress.percentage
            ),
            (
                "loading crate graph: tddy_desktop (lib)",
                "loading crate graph",
                64
            )
        );
        assert!(
            progress.furthest.contains("64%"),
            "the message must say how far the load got, was: {}",
            progress.furthest
        );
        assert!(
            !progress.ready,
            "a phase of a load in progress must not claim the index is ready"
        );
    }

    /// `phase` and `percentage` are the phase running now. rust-analyzer finishes "Roots Scanned"
    /// at 100% in seconds and then spends minutes building and indexing; a progress that kept
    /// naming the phase with the highest number read "Roots Scanned 100%" for the whole load.
    #[test]
    fn names_the_phase_running_now_rather_than_the_one_that_reached_the_highest_percentage() {
        // Given a server that scanned its roots to 100% and then began indexing
        let mut chatter = ServerChatter::unthrottled();
        let progress = |token: &str, value: serde_json::Value| json!({ "method": "$/progress", "params": { "token": token, "value": value } });
        chatter.absorb(&progress(
            "roots",
            json!({ "kind": "begin", "title": "Roots Scanned", "percentage": 0 }),
        ));
        chatter.absorb(&progress(
            "roots",
            json!({ "kind": "report", "percentage": 100 }),
        ));
        chatter.absorb(&progress("roots", json!({ "kind": "end" })));
        chatter.absorb(&progress(
            "index",
            json!({ "kind": "begin", "title": "Indexing", "percentage": 0 }),
        ));
        let line = chatter
            .absorb(&progress(
                "index",
                json!({ "kind": "report", "message": "tddy_core", "percentage": 37 }),
            ))
            .expect("an unthrottled report is a line");

        // When that line becomes a progress message
        let progress = reported_after(&chatter, line);

        // Then it names indexing at 37%, and the furthest any phase got is still carried
        assert_eq!(
            (progress.phase.as_str(), progress.percentage),
            ("Indexing", 37)
        );
        assert!(
            progress.furthest.contains("Roots Scanned 100%"),
            "{}",
            progress.furthest
        );
    }

    /// The deliberate answer to `NotificationEvent::Lost`: say it, keep waiting, and carry the
    /// furthest the load got. Dropping the gap in silence is how a wait ends unable to report
    /// where the server reached, which is the whole reason the phases are forwarded at all.
    #[test]
    fn says_how_many_progress_notifications_a_wait_lost_sight_of() {
        // Given a wait that fell behind a server which had reported 12%
        let mut chatter = ServerChatter::default();
        chatter.absorb(&json!({
            "method": "$/progress",
            "params": {
                "token": "load",
                "value": { "kind": "begin", "title": "loading crate graph", "percentage": 12 },
            }
        }));

        // When the loss is rendered
        let line = lost_sight_of(3, &chatter);

        // Then it names how many went, and where the load had got to
        assert!(
            line.contains("lost sight of 3 progress notification(s)"),
            "{line}"
        );
        assert!(line.contains("12%"), "{line}");
    }
}
