//! Warming a session's code index in the background once its worktree exists.
//!
//! [`warm_for_session`] asks the index daemon this runtime manages (`code_index.Warm`, over
//! [`IndexChannelSource::connect`] — the same forwarding path `code_navigation` uses, which for the
//! daemon's registry starts the index daemon on first use) to load the worktree's index, and drains
//! the answer. The warm is only a trigger: what the index is doing is per worktree root in the index
//! daemon, which `code_navigation.WatchCodeIndex` and the code pane's `CodeSession` follow through
//! `code_index.WatchRoot` — whoever started the load. Nothing is recorded here.
//!
//! Warming never blocks a session: it runs on its own task, a failure is logged and nothing else (the
//! root reports `FAILED` with its reason to whoever watches it), and a daemon without an
//! `index_daemon:` section warms nothing.

use std::path::Path;
use std::sync::Arc;

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;

use tddy_session_lifecycle::connection_service::SessionWorktreeObserver;

use crate::code_navigation::{index_daemon_error, IndexChannelSource};

/// Starts warming `worktree`'s code index for `session_id` in the background.
///
/// Returns the warm's task, or `None` when nothing was started: no index daemon is configured, or
/// `worktree` is not a Rust workspace (no `Cargo.toml` at its root). The task never fails the
/// session — a failed warm is logged.
#[must_use = "dropping the handle detaches the warm, it does not cancel it"]
pub fn warm_for_session(
    index_daemon: Option<&Arc<dyn IndexChannelSource>>,
    session_id: &str,
    worktree: &Path,
) -> Option<tokio::task::JoinHandle<()>> {
    let source = Arc::clone(index_daemon?);
    if !tddy_index_daemon::is_rust_workspace(worktree) {
        return None;
    }
    let session_id = session_id.to_string();
    let workspace_root = worktree.display().to_string();
    Some(tokio::spawn(async move {
        if let Err(reason) = warm(source.as_ref(), workspace_root).await {
            log::warn!("code index warm-up of session {session_id} failed: {reason}");
        }
    }))
}

/// Drains `workspace_root`'s `Warm`, ending when the stream does. An `Err` is the reason the warm
/// could not run, broke off, or ended short of `ready`.
async fn warm(source: &dyn IndexChannelSource, workspace_root: String) -> Result<(), String> {
    let channel = source
        .connect()
        .await
        .map_err(|err| index_daemon_error(&err))?;
    let stream = CodeIndexServiceClient::new(channel)
        .warm(index::WarmRequest { workspace_root })
        .await
        .map_err(warm_failed)?
        .into_inner();
    if drain_warm(stream).await.map_err(warm_failed)? {
        Ok(())
    } else {
        Err("warm: the index daemon ended the stream before the index was ready".to_string())
    }
}

/// Reads a `Warm` stream to its end — the warm is the trigger, its messages are not the progress
/// anyone reads — and answers whether its last message said the index is ready. An `Err` is the
/// status the stream broke off with.
pub(crate) async fn drain_warm(
    mut stream: tonic::Streaming<index::IndexProgress>,
) -> Result<bool, tonic::Status> {
    let mut ready = false;
    while let Some(update) = stream.message().await? {
        ready = update.ready;
    }
    Ok(ready)
}

/// A warm the index daemon refused or broke off, as one line.
fn warm_failed(status: tonic::Status) -> String {
    format!("warm: {}", status.message())
}

/// Starts a session's warm-up when its worktree appears: the daemon's end of the session host's
/// [`SessionWorktreeObserver`] port.
pub struct IndexWarmupObserver {
    index_daemon: Arc<dyn IndexChannelSource>,
}

impl IndexWarmupObserver {
    /// An observer warming through `index_daemon`.
    #[must_use]
    pub fn new(index_daemon: Arc<dyn IndexChannelSource>) -> Self {
        Self { index_daemon }
    }
}

impl SessionWorktreeObserver for IndexWarmupObserver {
    fn worktree_ready(&self, session_id: &str, worktree: &Path) {
        // Detached on purpose: a session's start never waits on its index.
        drop(warm_for_session(
            Some(&self.index_daemon),
            session_id,
            worktree,
        ));
    }
}
