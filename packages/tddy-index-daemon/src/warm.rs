//! What `Warm` waits for, and what it says while it waits.
//!
//! `ready` means the root's crate graph is loaded and queryable, which is what the schema always
//! claimed and what this RPC did not previously deliver: it returned as soon as
//! [`crate::index::WorkspaceIndex::client_for`] produced a live server. The gap was measured — a
//! real three-crate workspace warmed in 0.34s and then paid 2.1s on its first `Anchors`.
//!
//! A warm is "start the root's server, then follow the root until its graph is loaded": the
//! progress it forwards is the root's own ([`crate::graph::GraphLoad`], folded from the server's
//! first notification), so a warm joining a load already under way names every phase running
//! rather than reports whose `begin` it never saw, and `ready` is that progress's loaded latch.
//!
//! Three things end this task, and nothing else may: the graph loading, the server going away, and
//! the caller hanging up. A wait that cannot end is worse than one that ends badly.

use std::path::Path;

use tddy_rpc::Status;
use tokio::sync::watch;

use crate::activity::Activity;
use crate::graph::RootProgress;
use crate::index::WorkspaceIndex;
use crate::operations::{event_stream, EventSender};
use crate::proto::code_index::{IndexProgress, IndexState};
use crate::service::EventStream;
use crate::watch_root::reported;

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
        // The client is held for the whole wait, which is what keeps the server — and the progress
        // followed below — alive under this task rather than under whoever asked first.
        let _client = match index.client_for(&root).await {
            Ok(client) => client,
            Err(refusal) => {
                activity.refused(&refusal);
                let _ = events.send(Err(refusal)).await;
                return;
            }
        };
        // Attached by `client_for`, from the moment the server was reached. Absent only if no
        // server was ever reached for this root, which the arm above has already refused.
        let Some(graph) = index.graph_load_of(&root).await else {
            let refusal = Status::internal(format!(
                "no crate-graph watcher is attached to `{}`, so nothing can say whether its graph \
                 is loaded",
                root.display()
            ));
            activity.refused(&refusal);
            let _ = events.send(Err(refusal)).await;
            return;
        };

        match forward_until_loaded(graph.progress(), &events).await {
            Ending::Loaded(progress) => {
                let _ = events.send(Ok(ready(&root, &progress))).await;
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
    /// The server reported its graph queryable; the root's progress at that moment.
    Loaded(RootProgress),
    /// The server went away first, so nothing will ever report that graph loaded.
    ServerGone,
    /// Nobody is left to hear the answer.
    CallerGone,
}

/// Forward every change of the root's progress into `events` until its graph is loaded.
async fn forward_until_loaded(
    mut progress: watch::Receiver<RootProgress>,
    events: &EventSender<IndexProgress>,
) -> Ending {
    loop {
        let now = progress.borrow_and_update().clone();
        if now.loaded {
            return Ending::Loaded(now);
        }
        if events.send(Ok(reported(&now))).await.is_err() {
            return Ending::CallerGone;
        }
        tokio::select! {
            changed = progress.changed() => {
                if changed.is_err() {
                    return Ending::ServerGone;
                }
            }
            // The only disconnect signal this service has is a send that finds nobody listening,
            // and a warm of a cold root can go a long time between sends — so the channel's own
            // closing is watched too. Without it a caller could hang up during a six-minute load
            // and this task would wait on nothing until the graph finished.
            () = events.closed() => return Ending::CallerGone,
        }
    }
}

/// The first message of a warm: the root whose graph is about to be loaded.
fn loading(root: &Path) -> IndexProgress {
    IndexProgress {
        line: format!("loading the crate graph at {}", root.display()),
        state: IndexState::Loading as i32,
        ..IndexProgress::default()
    }
}

/// The last message of a warm: the graph is loaded and queryable.
fn ready(root: &Path, progress: &RootProgress) -> IndexProgress {
    IndexProgress {
        line: format!("{} is served from a warm index", root.display()),
        ready: true,
        ..reported(progress)
    }
}
