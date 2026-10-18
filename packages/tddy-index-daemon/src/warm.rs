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

use std::path::{Path, PathBuf};

use tddy_rpc::Status;
use tokio::sync::watch;

use crate::activity::Activity;
use crate::graph::RootProgress;
use crate::index::WorkspaceIndex;
use crate::operations::{event_stream, EventSender};
use crate::proto::code_index::{IndexProgress, IndexState};
use crate::service::EventStream;
use crate::watch_root::{next_change, reported, Gone};

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
    tokio::spawn(warm_root(index.clone(), root, events, activity));
    Ok(stream)
}

/// Start `root`'s server and report its progress into `events` until its graph is loaded, the
/// server goes away, or the caller leaves — and record which on `activity`.
async fn warm_root(
    index: WorkspaceIndex,
    root: PathBuf,
    events: EventSender<IndexProgress>,
    activity: Activity,
) {
    if events.send(Ok(loading(&root))).await.is_err() {
        activity.cancelled();
        return;
    }
    // The client is held for the whole wait, which is what keeps the server — and the progress
    // followed below — alive under this task rather than under whoever asked first.
    let _client = match index.client_for(&root).await {
        Ok(client) => client,
        Err(refusal) => return refuse(&activity, &events, refusal).await,
    };
    // Attached by `client_for`, from the moment the server was reached. Absent only if no
    // server was ever reached for this root, which the arm above has already refused.
    let Some(graph) = index.graph_load_of(&root).await else {
        let refusal = Status::internal(format!(
            "no crate-graph watcher is attached to `{}`, so nothing can say whether its graph \
             is loaded",
            root.display()
        ));
        return refuse(&activity, &events, refusal).await;
    };
    match forward_until_loaded(graph.progress(), &events).await {
        Ok(progress) => {
            let _ = events.send(Ok(ready(&root, &progress))).await;
            activity.answered();
        }
        Err(Gone::Server) => {
            let refusal = Status::unavailable(format!(
                "the language server holding `{}` went away before its crate graph was loaded",
                root.display()
            ));
            refuse(&activity, &events, refusal).await;
        }
        Err(Gone::Caller) => activity.cancelled(),
    }
}

/// End a warm with `refusal`: recorded on `activity`, and sent to the caller if it is still there.
async fn refuse(activity: &Activity, events: &EventSender<IndexProgress>, refusal: Status) {
    activity.refused(&refusal);
    let _ = events.send(Err(refusal)).await;
}

/// Forward every change of the root's progress into `events` until its graph is loaded — the
/// root's progress at that moment — or until the server or the caller is gone.
async fn forward_until_loaded(
    mut progress: watch::Receiver<RootProgress>,
    events: &EventSender<IndexProgress>,
) -> Result<RootProgress, Gone> {
    loop {
        let now = progress.borrow_and_update().clone();
        if now.loaded {
            return Ok(now);
        }
        if events.send(Ok(reported(&now))).await.is_err() {
            return Err(Gone::Caller);
        }
        next_change(&mut progress, events).await?;
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
