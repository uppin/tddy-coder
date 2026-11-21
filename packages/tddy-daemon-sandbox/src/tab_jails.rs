//! The jails a sandboxed session's **shell tabs** run in, one per tab (`#session-liveness` 5/8).
//!
//! A session's main jail is [`SandboxSessionManager`](crate::sandbox_session::SandboxSessionManager)'s,
//! one per session. A tab opened "In sandbox" gets a jail of its own — a second
//! `tddy-sandbox-runner` over the same worktree and sandbox grants, with files of its own
//! ([`TabJailPaths`]) — so the tab's exit or crash never takes the main's jail with it.
//!
//! [`TabJails`] is the registry of those jails, keyed `(session_id, terminal_id)`. It starts a jail
//! through a [`TabJailSpawn`] port — the daemon's lowers the session's grants and spawns the runner;
//! a test's is an in-memory double — and **watches each for death**: a jail whose runner exits is
//! forgotten and announced on [`TabJails::subscribe_ended`], so the tab it served ends with it.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::Status;
use tokio::sync::{broadcast, Mutex};

use crate::sandbox_session::SandboxSessionState;
pub use crate::tab_jail_paths::TabJailPaths;

/// How many jail ends [`TabJails::subscribe_ended`] buffers for a subscriber that has fallen behind.
const ENDED_CAPACITY: usize = 64;

/// The tab a jail serves: its session and its terminal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabJailKey {
    pub session_id: String,
    pub terminal_id: String,
}

impl TabJailKey {
    pub fn new(session_id: &str, terminal_id: &str) -> Self {
        TabJailKey {
            session_id: session_id.to_string(),
            terminal_id: terminal_id.to_string(),
        }
    }
}

/// What one shell tab's jail is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabJailLaunch {
    pub session_id: String,
    pub terminal_id: String,
    /// The session's directory: its record names the grants the jail is lowered from, and its
    /// `sandbox/tabs/<terminal_id>/` holds the jail's files.
    pub session_dir: PathBuf,
    /// The session's worktree, mounted in the jail and the shell's working directory.
    pub worktree: PathBuf,
    /// The jail's own files, distinct from the main jail's and every other tab's.
    pub paths: TabJailPaths,
}

impl TabJailLaunch {
    /// The launch of the jail of tab `terminal_id` of session `session_id`, whose directory is
    /// `session_dir` and whose worktree is `worktree`; `INVALID_ARGUMENT` when the terminal id cannot
    /// name a directory of the session ([`TabJailPaths::for_tab`]).
    pub fn for_tab(
        session_id: &str,
        terminal_id: &str,
        session_dir: &Path,
        worktree: &Path,
    ) -> Result<TabJailLaunch, Status> {
        Ok(TabJailLaunch {
            session_id: session_id.to_string(),
            terminal_id: terminal_id.to_string(),
            session_dir: session_dir.to_path_buf(),
            worktree: worktree.to_path_buf(),
            paths: TabJailPaths::for_tab(session_dir, session_id, terminal_id)?,
        })
    }

    pub fn key(&self) -> TabJailKey {
        TabJailKey::new(&self.session_id, &self.terminal_id)
    }
}

/// Starts the jail a shell tab runs in.
///
/// A port because starting one needs what only the daemon has — the session's tool definitions,
/// lowered into its grants, the runner binary and the host relay — and because the registry's
/// contracts (one jail per tab, every teardown stops it, an exited one ends its tab) are testable
/// without a kernel sandbox.
#[async_trait]
pub trait TabJailSpawn: Send + Sync {
    /// Start a jail running the session user's login shell in `launch.worktree`, with the
    /// session's sandbox grants and `launch.paths` as its files, and return it once its runner is
    /// ready and its terminal bridged. A jail that cannot start is an error naming why, and leaves
    /// nothing running.
    async fn spawn_shell(&self, launch: &TabJailLaunch)
        -> Result<Arc<SandboxSessionState>, Status>;
}

/// The jails of every jailed tab this daemon runs, keyed `(session_id, terminal_id)`.
pub struct TabJails {
    jails: Mutex<HashMap<TabJailKey, Arc<SandboxSessionState>>>,
    /// Every jail whose runner exited on its own: the death watch's announcements.
    ended: broadcast::Sender<TabJailKey>,
}

impl Default for TabJails {
    fn default() -> Self {
        TabJails {
            jails: Mutex::new(HashMap::new()),
            ended: broadcast::channel(ENDED_CAPACITY).0,
        }
    }
}

impl TabJails {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start the jail `launch` describes through `spawn`, register it under its tab, and watch it:
    /// once its runner exits it is forgotten and announced on [`Self::subscribe_ended`]. A tab that
    /// already has a jail is `ALREADY_EXISTS`, and nothing is started; a refused spawn registers
    /// nothing.
    pub async fn start(
        &self,
        spawn: &dyn TabJailSpawn,
        launch: TabJailLaunch,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        // TODO(sandbox-shell-tab): implement
        let _ = (spawn, launch);
        todo!("start and watch a tab's jail")
    }

    /// The jail of tab `terminal_id` of `session_id`, while it is registered.
    pub async fn get(
        &self,
        session_id: &str,
        terminal_id: &str,
    ) -> Option<Arc<SandboxSessionState>> {
        self.jails
            .lock()
            .await
            .get(&TabJailKey::new(session_id, terminal_id))
            .cloned()
    }

    /// Stop the jail of tab `terminal_id` of `session_id` and forget it. `false` when the tab has no
    /// jail. A jail stopped here is not announced as ended: its stop was asked for.
    pub async fn stop(&self, session_id: &str, terminal_id: &str) -> bool {
        // TODO(sandbox-shell-tab): implement
        let _ = (session_id, terminal_id);
        todo!("stop a tab's jail")
    }

    /// Stop every tab jail of `session_id` and forget them; the terminal ids stopped, sorted. Every
    /// other session's jails are left running.
    pub async fn stop_session(&self, session_id: &str) -> Vec<String> {
        // TODO(sandbox-shell-tab): implement
        let _ = session_id;
        todo!("stop a session's tab jails")
    }

    /// Stop every tab jail this daemon runs and forget them — a daemon shutting down leaves none
    /// behind; the tabs stopped, sorted.
    pub async fn stop_all(&self) -> Vec<TabJailKey> {
        // TODO(sandbox-shell-tab): implement
        todo!("stop every tab jail")
    }

    /// The terminal ids of `session_id` whose jail's runner is running now.
    pub async fn live(&self, session_id: &str) -> BTreeSet<String> {
        // TODO(sandbox-shell-tab): implement
        let _ = session_id;
        todo!("the live tab jails of a session")
    }

    /// The death watch: every tab whose jail's runner exited on its own from now on, once each.
    pub fn subscribe_ended(&self) -> broadcast::Receiver<TabJailKey> {
        self.ended.subscribe()
    }
}

#[cfg(all(test, unix))]
#[path = "tab_jails_tests.rs"]
mod tests;
