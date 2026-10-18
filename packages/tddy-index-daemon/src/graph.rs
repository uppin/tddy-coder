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

use crate::proto::code_index::IndexState;

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

/// The state the wire reports for a server whose latest status said `quiescent`: `READY` when it
/// did, `LOADING` otherwise — the one reading of quiescence, for a follower and an answer alike.
pub(crate) fn state_of_quiescence(quiescent: bool) -> IndexState {
    if quiescent {
        IndexState::Ready
    } else {
        IndexState::Loading
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
/// exists instead of — and says so in the published line, with the furthest the load got. A phase
/// whose `end` may have been lost is dropped once the server next says it is quiescent, unless it
/// has reported since ([`ServerChatter::lost_sight`]); without that it would be listed for the
/// server's life.
async fn fold_the_servers_progress(
    mut notifications: NotificationStream,
    published: watch::Sender<RootProgress>,
) {
    let mut chatter = ServerChatter::unthrottled();
    loop {
        match hear(&mut chatter, notifications.recv().await) {
            Heard::Line(line) => {
                published.send_if_modified(|progress| progress.fold(&chatter, line));
            }
            // Dropping the sender is the signal: every follower learns the server went away
            // instead of waiting on a transition that can no longer happen.
            Heard::ServerGone => return,
        }
    }
}

/// What one notification event means for a root's progress.
#[derive(Debug, PartialEq, Eq)]
enum Heard {
    /// Fold the chatter in, with this line replacing the last one when there is one.
    Line(Option<String>),
    /// The client is gone, so nothing will ever report on this server again.
    ServerGone,
}

/// Fold `event` into `chatter`, and say what it means for the root's progress.
fn hear(chatter: &mut ServerChatter, event: NotificationEvent) -> Heard {
    match event {
        NotificationEvent::Received(notification) => Heard::Line(chatter.absorb(&notification)),
        NotificationEvent::Lost(lost) => {
            log::debug!(
                target: "tddy_index_daemon::graph",
                "lost sight of {lost} notification(s) while following a root's progress"
            );
            chatter.lost_sight();
            Heard::Line(Some(lost_sight_of(lost, chatter)))
        }
        NotificationEvent::Ended => Heard::ServerGone,
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
    use crate::proto::code_index::IndexProgress;
    use crate::proto::code_index::IndexState;
    use serde_json::{json, Value};

    /// What a watcher of the root is sent once `chatter`'s reading and `line` are folded in.
    fn reported_after(chatter: &ServerChatter, line: String) -> IndexProgress {
        let mut progress = RootProgress::default();
        progress.fold(chatter, Some(line));
        crate::watch_root::reported(&progress)
    }

    /// The root's progress once its watcher has heard `events`, in order, from a fresh server.
    fn progress_after(events: Vec<NotificationEvent>) -> RootProgress {
        let mut chatter = ServerChatter::unthrottled();
        let mut progress = RootProgress::default();
        for event in events {
            if let Heard::Line(line) = hear(&mut chatter, event) {
                progress.fold(&chatter, line);
            }
        }
        progress
    }

    /// The state a watcher of `progress` reads.
    fn the_state_of(progress: &RootProgress) -> IndexState {
        IndexState::try_from(crate::watch_root::reported(progress).state)
            .expect("a known index state")
    }

    fn progress(token: &str, value: Value) -> Value {
        json!({ "method": "$/progress", "params": { "token": token, "value": value } })
    }

    fn a_begin(token: &str, title: &str) -> NotificationEvent {
        NotificationEvent::Received(progress(token, json!({ "kind": "begin", "title": title })))
    }

    fn a_report(token: &str, message: &str, percentage: u64) -> NotificationEvent {
        NotificationEvent::Received(progress(
            token,
            json!({ "kind": "report", "message": message, "percentage": percentage }),
        ))
    }

    fn a_status(quiescent: bool) -> NotificationEvent {
        NotificationEvent::Received(json!({
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": quiescent }
        }))
    }

    fn a_phase(title: &str, message: &str, percentage: Option<u64>) -> ActivePhase {
        ActivePhase {
            title: title.to_string(),
            message: message.to_string(),
            percentage,
        }
    }

    /// The state is read from the server's latest `serverStatus`. A server that has begun work but
    /// said nothing of its status yet is loading, not idle and not ready.
    #[test]
    fn a_root_whose_server_has_sent_no_status_reads_loading() {
        // Given a server that began indexing and has sent no status
        let events = vec![a_begin("index", "Indexing")];

        // When its progress is folded
        let progress = progress_after(events);

        // Then a watcher reads it loading
        assert_eq!(the_state_of(&progress), IndexState::Loading);
    }

    /// Not a latch: an edit that sends the server back to work reads as loading again, while the
    /// loaded latch — what `Warm` ends on — stays set.
    #[test]
    fn a_ready_root_reads_loading_again_when_its_server_goes_busy() {
        // Given a server that reported itself quiescent
        let mut events = vec![a_status(true)];

        // When it then reports itself busy again
        events.push(a_status(false));
        let progress = progress_after(events);

        // Then a watcher reads it loading, though its graph stays loaded
        assert_eq!(
            (the_state_of(&progress), progress.loaded),
            (IndexState::Loading, true)
        );
    }

    /// A gap in the notifications can swallow a phase's `end`. Without dropping it, the
    /// phase would be listed beside a ready index for the server's whole life.
    #[test]
    fn a_phase_whose_end_was_lost_in_a_gap_is_gone_once_the_server_is_quiescent() {
        // Given a watcher that lost sight of notifications while fetching was in flight
        let mut events = vec![a_begin("fetch", "Fetching"), NotificationEvent::Lost(3)];

        // When the server then says it is quiescent
        events.push(a_status(true));
        let progress = progress_after(events);

        // Then the root reads ready with nothing running
        assert_eq!(
            (the_state_of(&progress), progress.phases),
            (IndexState::Ready, vec![])
        );
    }

    #[test]
    fn a_phase_that_reported_after_a_gap_is_still_listed_once_the_server_is_quiescent() {
        // Given a watcher that lost sight of notifications while indexing was in flight, and heard
        // from indexing again afterwards
        let mut events = vec![
            a_begin("index", "Indexing"),
            NotificationEvent::Lost(2),
            a_report("index", "300/310 (subject)", 97),
        ];

        // When the server then says it is quiescent
        events.push(a_status(true));
        let progress = progress_after(events);

        // Then indexing is still listed: it proved itself running
        assert_eq!(
            progress.phases,
            vec![a_phase("Indexing", "300/310 (subject)", Some(97))]
        );
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
            &progress(
                "load",
                json!({ "kind": "begin", "title": "loading crate graph" }),
            ),
            started,
        );
        chatter.absorb_at(
            &progress("load", json!({ "kind": "report", "percentage": 64 })),
            after(1),
        );
        let line = chatter
            .absorb_at(
                &progress(
                    "load",
                    json!({ "kind": "report", "message": "tddy_desktop (lib)" }),
                ),
                after(3),
            )
            .expect("a sub-step, once the interval has passed, is worth a line");

        // When that line becomes a progress message
        let reported = reported_after(&chatter, line);

        // Then the phase and the percentage are the furthest the load got, the line is the
        // sub-step that arrived, and nothing claims the index ready
        assert_eq!(
            (
                reported.line.as_str(),
                reported.phase.as_str(),
                reported.percentage,
                reported.furthest.as_str(),
                reported.ready,
            ),
            (
                "loading crate graph: tddy_desktop (lib)",
                "loading crate graph",
                64,
                "loading crate graph: tddy_desktop (lib); furthest loading crate graph 64%",
                false,
            )
        );
    }

    /// `phase` and `percentage` are the phase running now. rust-analyzer finishes "Roots Scanned"
    /// at 100% in seconds and then spends minutes building and indexing; a progress that kept
    /// naming the phase with the highest number read "Roots Scanned 100%" for the whole load.
    #[test]
    fn names_the_phase_running_now_rather_than_the_one_that_reached_the_highest_percentage() {
        // Given a server that scanned its roots to 100% and then began indexing
        let mut chatter = ServerChatter::unthrottled();
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
        let reported = reported_after(&chatter, line);

        // Then it names indexing at 37%, and the furthest any phase got is still carried
        assert_eq!(
            (
                reported.phase.as_str(),
                reported.percentage,
                reported.furthest.as_str()
            ),
            (
                "Indexing",
                37,
                "Indexing: tddy_core (37%); furthest Roots Scanned 100%"
            )
        );
    }

    /// The deliberate answer to `NotificationEvent::Lost`: say it, keep waiting, and carry the
    /// furthest the load got. Dropping the gap in silence is how a wait ends unable to report
    /// where the server reached, which is the whole reason the phases are forwarded at all.
    #[test]
    fn says_how_many_progress_notifications_a_wait_lost_sight_of() {
        // Given a wait that fell behind a server which had reported 12%
        let mut chatter = ServerChatter::default();
        chatter.absorb(&progress(
            "load",
            json!({ "kind": "begin", "title": "loading crate graph", "percentage": 12 }),
        ));

        // When the loss is rendered
        let line = lost_sight_of(3, &chatter);

        // Then it names how many went, and where the load had got to
        assert_eq!(
            line,
            "lost sight of 3 progress notification(s) the server sent faster than this wait read \
             them; furthest so far: loading crate graph (12%); furthest loading crate graph 12%"
        );
    }
}
