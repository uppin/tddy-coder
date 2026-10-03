//! Warming a session's code index in the background, and keeping its latest progress for the
//! session header's indexing indicator.
//!
//! Once a session's worktree exists, [`warm_for_session`] asks the index daemon this runtime manages
//! (`code_index.Warm`, over [`IndexChannelSource::connect`] — the same forwarding path
//! `code_navigation` uses, which for the daemon's registry starts the index daemon on first use) to
//! load the worktree's index, and records every `IndexProgress` it streams in a [`SessionIndexProgress`] under the session's
//! id. `code_navigation.WatchCodeIndex` reads that holder.
//!
//! Warming never blocks a session: it runs on its own task, a failure is recorded as the session's
//! last progress (with `error` set) and nothing else, and a daemon without an `index_daemon:` section
//! warms nothing. "Ready" is `Warm`'s own `ready` — the graph is loaded — not the registry's
//! socket-bound readiness (`docs/dev/todo/2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tddy_service::proto::code_navigation::CodeIndexProgress;
use tokio::sync::watch;

use crate::code_navigation::IndexChannelSource;

/// The latest code-index progress of every session something warmed, each observable as it changes.
///
/// One `watch` channel per session: a watcher arriving late sees the latest message rather than the
/// history, which is all an indicator shows.
#[derive(Clone, Default)]
pub struct SessionIndexProgress {
    sessions: Arc<Mutex<HashMap<String, watch::Sender<Option<CodeIndexProgress>>>>>,
}

impl SessionIndexProgress {
    /// An empty holder: no session has been warmed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `progress` as `session_id`'s latest.
    pub fn record(&self, session_id: &str, progress: CodeIndexProgress) {
        self.sender_for(session_id).send_replace(Some(progress));
    }

    /// `session_id`'s latest progress, or `None` when nothing has warmed it.
    #[must_use]
    pub fn latest(&self, session_id: &str) -> Option<CodeIndexProgress> {
        self.sessions
            .lock()
            .expect("the session progress map")
            .get(session_id)
            .and_then(|sender| sender.borrow().clone())
    }

    /// Follows `session_id`'s progress from its latest value on; `None` until something records one.
    #[must_use]
    pub fn watch(&self, session_id: &str) -> watch::Receiver<Option<CodeIndexProgress>> {
        self.sender_for(session_id).subscribe()
    }

    fn sender_for(&self, session_id: &str) -> watch::Sender<Option<CodeIndexProgress>> {
        self.sessions
            .lock()
            .expect("the session progress map")
            .entry(session_id.to_string())
            .or_insert_with(|| watch::channel(None).0)
            .clone()
    }
}

/// Starts warming `worktree`'s code index for `session_id` in the background, recording its
/// progress in `progress`.
///
/// Returns the warm's task, or `None` when nothing was started: no index daemon is configured, or
/// `worktree` is not a Rust workspace (no `Cargo.toml` at its root). The task never fails the
/// session — a failed warm ends with one [`CodeIndexProgress`] whose `error` says why.
#[must_use = "dropping the handle detaches the warm, it does not cancel it"]
pub fn warm_for_session(
    index_daemon: Option<&Arc<dyn IndexChannelSource>>,
    progress: &SessionIndexProgress,
    session_id: &str,
    worktree: &Path,
) -> Option<tokio::task::JoinHandle<()>> {
    // TODO(indexing-indicators): with an index channel source and a `Cargo.toml` at `worktree`'s root, spawn a
    // task that `connect`s (mapping the port's error to `error`), calls `code_index.Warm { workspace_root: worktree }`, records each
    // `IndexProgress` as `CodeIndexProgress`, and records a failed connect or warm as `error`.
    let _ = (index_daemon, progress, session_id, worktree);
    None
}
