//! What `Warm` waits for, and what it says while it waits.
//!
//! `ready` means the root's crate graph is loaded and queryable, which is what the schema always
//! claimed and what this RPC did not previously deliver: it returned as soon as
//! [`crate::index::WorkspaceIndex::client_for`] produced a live server. The gap was measured — a
//! real three-crate workspace warmed in 0.34s and then paid 2.1s on its first `Anchors`.
//!
//! So two things happen here. The server's own phases are forwarded into the stream, because a
//! wait of minutes with nothing to read is indistinguishable from a hang; and the wait ends on
//! [`crate::graph::GraphLoad`], the per-root latch fed by the server's own
//! `experimental/serverStatus`.
//!
//! Three things end this task, and nothing else may: the graph loading, the server going away, and
//! the caller hanging up. A wait that cannot end is worse than one that ends badly.

use std::path::Path;

use tddy_code_restructuring::backends::rust::ServerChatter;
use tddy_lsp::{NotificationEvent, NotificationStream};
use tddy_rpc::Status;

use crate::activity::Activity;
use crate::index::WorkspaceIndex;
use crate::operations::{event_stream, EventSender};
use crate::proto::code_index::IndexProgress;
use crate::service::EventStream;

/// Load a workspace root's crate graph and report progress until it is queryable.
///
/// Idempotent, as the schema says: a root whose graph this process has already observed loaded is
/// answered straight away, because a server reports itself quiescent on the transition and never
/// again — a second warm that waited to be told would wait for ever.
pub(crate) async fn serve_warm(
    index: &WorkspaceIndex,
    workspace_root: &str,
) -> Result<EventStream<IndexProgress>, Status> {
    let (activity, root) = Activity::arrived("warm", index, workspace_root).await?;
    let (events, stream) = event_stream();
    let index = index.clone();

    tokio::spawn(async move {
        if events.send(Ok(loading(&root))).await.is_err() {
            activity.cancelled();
            return;
        }
        // The client is held for the whole wait, which is what keeps the reader loop feeding the
        // notifications below alive under this task rather than under whoever asked first.
        let client = match index.client_for(&root).await {
            Ok(client) => client,
            Err(refusal) => {
                activity.refused(&refusal);
                let _ = events.send(Err(refusal)).await;
                return;
            }
        };
        // Attached by `client_for`, from the moment the server was reached. Absent only if no
        // server was ever reached for this root, which the arm above has already refused.
        let Some(mut graph) = index.graph_load_of(&root).await else {
            let refusal = Status::internal(format!(
                "no crate-graph watcher is attached to `{}`, so nothing can say whether its graph \
                 is loaded",
                root.display()
            ));
            activity.refused(&refusal);
            let _ = events.send(Err(refusal)).await;
            return;
        };
        let notifications = client.subscribe_notifications();

        match narrate_until_loaded(notifications, &mut graph, &events).await {
            Ending::Loaded => {
                let _ = events.send(Ok(ready(&root))).await;
                activity.answered();
            }
            Ending::ServerGone => {
                let refusal = Status::unavailable(format!(
                    "the language server holding `{}` went away before its crate graph was loaded",
                    root.display()
                ));
                activity.refused(&refusal);
                let _ = events.send(Err(refusal)).await;
            }
            Ending::CallerGone => activity.cancelled(),
        }
    });

    Ok(stream)
}

/// How a warm stopped waiting.
#[derive(Debug, PartialEq, Eq)]
enum Ending {
    /// The server reported its graph queryable.
    Loaded,
    /// The server went away first, so nothing will ever report that graph loaded.
    ServerGone,
    /// Nobody is left to hear the answer.
    CallerGone,
}

/// Forward the server's phases into `events` until its graph is loaded.
async fn narrate_until_loaded(
    mut notifications: NotificationStream,
    graph: &mut crate::graph::GraphLoad,
    events: &EventSender<IndexProgress>,
) -> Ending {
    let mut chatter = ServerChatter::default();
    loop {
        if graph.is_loaded() {
            return Ending::Loaded;
        }
        let phase = tokio::select! {
            // A load that finishes between two notifications must end the wait, so the latch is
            // watched alongside the narration rather than only polled above.
            loaded = graph.loaded() => {
                return if loaded { Ending::Loaded } else { Ending::ServerGone };
            }
            // The only disconnect signal this service has is a send that finds nobody listening,
            // and a warm of a cold root can go a long time between sends — so the channel's own
            // closing is watched too. Without it a caller could hang up during a six-minute load
            // and this task would narrate to nothing until the graph finished.
            () = events.closed() => return Ending::CallerGone,
            event = notifications.recv() => event,
        };
        let line = match phase {
            NotificationEvent::Received(notification) => chatter.absorb(&notification),
            // Said rather than skipped. A wait that loses sight of the server and does not mention
            // it is how a slow load becomes indistinguishable from a hang — and the furthest the
            // load got is still carried, which is the one thing a reader of this can act on.
            NotificationEvent::Lost(lost) => Some(lost_sight_of(lost, &chatter)),
            NotificationEvent::Ended => {
                return if graph.is_loaded() {
                    Ending::Loaded
                } else {
                    Ending::ServerGone
                }
            }
        };
        if let Some(line) = line {
            if events.send(Ok(phase_of(&line, &chatter))).await.is_err() {
                return Ending::CallerGone;
            }
        }
    }
}

/// The first message of a warm: the root whose graph is about to be loaded.
fn loading(root: &Path) -> IndexProgress {
    IndexProgress {
        line: format!("loading the crate graph at {}", root.display()),
        ..IndexProgress::default()
    }
}

/// The last message of a warm: the graph is loaded and queryable.
fn ready(root: &Path) -> IndexProgress {
    IndexProgress {
        line: format!("{} is served from a warm index", root.display()),
        ready: true,
        ..IndexProgress::default()
    }
}

/// One phase the server reported, with where the load has got to so far.
///
/// `phase` and `percentage` are the *furthest* the load has reached rather than whatever the last
/// notification happened to carry, because the server routinely counts files inside a phase and
/// then emits sub-steps with no percentage at all — so the last number is not the highest one, and
/// a client watching the field go backwards would read a load in progress as one going wrong.
fn phase_of(line: &str, chatter: &ServerChatter) -> IndexProgress {
    let (percentage, phase) = chatter.furthest().unwrap_or((0, ""));
    IndexProgress {
        line: line.to_string(),
        phase: phase.to_string(),
        percentage: percentage as u32,
        furthest: chatter.how_far(),
        ready: false,
    }
}

/// What the stream says when this wait fell behind the server it is watching.
///
/// Reported as a line of its own, with the furthest the load had got attached, and the wait carries
/// on: the graph is no less likely to be loading for the gap. Silence here is the failure this
/// account exists to replace — a warm that lost the progress it was forwarding and said nothing
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

    /// A phase's percentage must be the furthest the load reached, not the last number to arrive.
    /// The server counts files inside a phase and then emits sub-steps with no percentage at all,
    /// so the last notification routinely has no number in it — and a `percentage` that went
    /// backwards would read as a load going wrong rather than one still going.
    #[test]
    fn carries_the_furthest_the_load_reached_rather_than_the_last_number_to_arrive() {
        // Given a server that reported 64% and then a sub-step carrying no percentage
        let mut chatter = ServerChatter::default();
        chatter.absorb(&json!({
            "method": "$/progress",
            "params": {
                "token": "load",
                "value": { "kind": "begin", "title": "loading crate graph" },
            }
        }));
        chatter.absorb(&json!({
            "method": "$/progress",
            "params": { "token": "load", "value": { "kind": "report", "percentage": 64 } }
        }));
        let line = chatter
            .absorb(&json!({
                "method": "$/progress",
                "params": {
                    "token": "load",
                    "value": { "kind": "report", "message": "tddy_desktop (lib)" },
                }
            }))
            .expect("a sub-step is worth a line");

        // When that line becomes a progress message
        let progress = phase_of(&line, &chatter);

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
