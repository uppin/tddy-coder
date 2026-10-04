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

use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::tonic_code_index::code_index_service_client::CodeIndexServiceClient;
use tddy_service::proto::code_navigation::CodeIndexProgress;
use tokio::sync::watch;

use tddy_session_lifecycle::connection_service::SessionWorktreeObserver;

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

    /// Follows `session_id`'s progress from its latest value on, or `None` when nothing has warmed
    /// it. It never creates an entry, so following an id nothing warmed (or one the caller does
    /// not own) leaves no trace.
    #[must_use]
    pub fn follow(&self, session_id: &str) -> Option<watch::Receiver<Option<CodeIndexProgress>>> {
        self.sessions
            .lock()
            .expect("the session progress map")
            .get(session_id)
            .map(watch::Sender::subscribe)
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
    let source = Arc::clone(index_daemon?);
    if !worktree.join("Cargo.toml").is_file() {
        return None;
    }
    let progress = progress.clone();
    // Recorded before the task starts, so a watcher arriving first finds a warm under way rather
    // than a session nothing warmed.
    progress.record(
        session_id,
        CodeIndexProgress {
            line: "Starting the code index".to_string(),
            phase: "Starting".to_string(),
            ..Default::default()
        },
    );
    let session_id = session_id.to_string();
    let workspace_root = worktree.display().to_string();
    Some(tokio::spawn(async move {
        if let Err(reason) = warm(source.as_ref(), &progress, &session_id, workspace_root).await {
            log::warn!("code index warm-up of session {session_id} failed: {reason}");
            progress.record(
                &session_id,
                CodeIndexProgress {
                    error: reason,
                    ..Default::default()
                },
            );
        }
    }))
}

/// Streams `workspace_root`'s `Warm` into `progress`, ending when the stream does. An `Err` is the
/// reason the warm could not run, broke off, or ended short of `ready`.
async fn warm(
    source: &dyn IndexChannelSource,
    progress: &SessionIndexProgress,
    session_id: &str,
    workspace_root: String,
) -> Result<(), String> {
    let channel = source
        .connect()
        .await
        .map_err(|err| format!("index daemon: {err}"))?;
    let mut stream = CodeIndexServiceClient::new(channel)
        .warm(index::WarmRequest { workspace_root })
        .await
        .map_err(|status| format!("warm: {}", status.message()))?
        .into_inner();
    let mut ready = false;
    while let Some(update) = stream
        .message()
        .await
        .map_err(|status| format!("warm: {}", status.message()))?
    {
        ready = update.ready;
        progress.record(session_id, web_progress(update));
    }
    if ready {
        Ok(())
    } else {
        Err("warm: the index daemon ended the stream before the index was ready".to_string())
    }
}

fn web_progress(update: index::IndexProgress) -> CodeIndexProgress {
    CodeIndexProgress {
        line: update.line,
        phase: update.phase,
        percentage: update.percentage,
        furthest: update.furthest,
        ready: update.ready,
        error: String::new(),
    }
}

/// Starts a session's warm-up when its worktree appears: the daemon's end of the session host's
/// [`SessionWorktreeObserver`] port.
pub struct IndexWarmupObserver {
    index_daemon: Arc<dyn IndexChannelSource>,
    progress: SessionIndexProgress,
}

impl IndexWarmupObserver {
    /// An observer warming through `index_daemon` and recording into `progress`.
    #[must_use]
    pub fn new(index_daemon: Arc<dyn IndexChannelSource>, progress: SessionIndexProgress) -> Self {
        Self {
            index_daemon,
            progress,
        }
    }
}

impl SessionWorktreeObserver for IndexWarmupObserver {
    fn worktree_ready(&self, session_id: &str, worktree: &Path) {
        // Detached on purpose: a session's start never waits on its index.
        drop(warm_for_session(
            Some(&self.index_daemon),
            &self.progress,
            session_id,
            worktree,
        ));
    }
}
