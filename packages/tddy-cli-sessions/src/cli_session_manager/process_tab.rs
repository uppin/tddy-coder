//! A **process tab**: a session tab with no PTY — a tool session's `tddy-coder`, held by a spawn
//! backend rather than by this manager (`#session-liveness` 3/4).
//!
//! It is the session's `main` tab, so code asking whether main is served keeps working for tool
//! sessions. It counts as a live agent ([`CliSessionManager::live_agent_sessions`]), it is not
//! listed by [`CliSessionManager::list_terminals`] (it has no terminal to stream), and it leaves the
//! registry when its [`ExitReceiver`] resolves. Stopping and signalling it go to its
//! [`ProcessTabHolder`] — the holder owns the child, so the pid is valid until the holder reports the
//! exit, which the pid a session's `.session.yaml` records never is.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tddy_task::ExitReceiver;
use tokio::sync::RwLock;

use super::{CliSessionManager, MAIN_TERMINAL_ID};

/// The tab kind of a tool session's `tddy-coder`.
pub const TDDY_CODER_KIND: &str = "tddy-coder";

/// Whoever holds a process tab's child: the forked spawn worker, the supervisor, or a direct spawn's
/// wait thread. Implemented beside each spawn backend.
#[async_trait::async_trait]
pub trait ProcessTabHolder: Send + Sync {
    /// Ask the holder to stop the child. Returns once asked; the child's end is reported through
    /// the tab's [`ExitReceiver`].
    async fn stop(&self) -> anyhow::Result<()>;

    /// Deliver `signal` (a `libc` signal number) to the child.
    async fn signal(&self, signal: i32) -> anyhow::Result<()>;
}

/// A process tab as the registry holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTab {
    pub terminal_id: String,
    pub kind: String,
    pub pid: u32,
}

/// Why [`CliSessionManager::signal_process_tab`] delivered nothing.
#[derive(Debug)]
pub enum SignalRefused {
    /// The session has no process tab: its child was never held, or has ended.
    NotHeld,
    /// The holder could not deliver it.
    Holder(anyhow::Error),
}

/// A process tab and who holds its child. `generation` tells one registration of a session's tab
/// from the next, so an exit that resolves late never removes the tab of a resumed child.
pub(super) struct ProcessTabEntry {
    generation: u64,
    tab: ProcessTab,
    holder: Arc<dyn ProcessTabHolder>,
}

/// `session_id → its process tab`.
pub(super) type ProcessTabs = Arc<RwLock<HashMap<String, ProcessTabEntry>>>;

/// Hands out the generation of each registration.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

impl CliSessionManager {
    /// Hold `session_id`'s child `pid` as its `main` tab of `kind`, until `exit` resolves. `holder`
    /// is what stops and signals it.
    ///
    /// A session that already holds a process tab (its child ended and was resumed before the old
    /// exit was read) is given the new one. A session whose `main` is a terminal is refused: there
    /// is one `main` tab.
    pub async fn register_process_tab(
        &self,
        session_id: &str,
        kind: &str,
        pid: u32,
        exit: ExitReceiver,
        holder: Arc<dyn ProcessTabHolder>,
    ) -> anyhow::Result<()> {
        if self.get(session_id).await.is_some() {
            anyhow::bail!("session {session_id} already holds a terminal as its main tab");
        }
        let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        self.process_tabs.write().await.insert(
            session_id.to_string(),
            ProcessTabEntry {
                generation,
                tab: ProcessTab {
                    terminal_id: MAIN_TERMINAL_ID.to_string(),
                    kind: kind.to_string(),
                    pid,
                },
                holder,
            },
        );
        let tabs = Arc::clone(&self.process_tabs);
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            // A holder that went away reports nothing, so the tab goes with it: the daemon has no
            // knowledge of the child left to hold the session live on.
            match exit.exited().await {
                Ok(end) => log::info!(
                    target: "tddy_daemon::claude_cli_session",
                    "process tab ended: session={session_id} pid={pid} exit={end:?}"
                ),
                Err(gone) => log::warn!(
                    target: "tddy_daemon::claude_cli_session",
                    "process tab ended: session={session_id} pid={pid}: {gone}"
                ),
            }
            let mut tabs = tabs.write().await;
            if tabs
                .get(&session_id)
                .is_some_and(|entry| entry.generation == generation)
            {
                tabs.remove(&session_id);
            }
        });
        Ok(())
    }

    /// The process tab `session_id` holds, if any.
    pub async fn process_tab(&self, session_id: &str) -> Option<ProcessTab> {
        self.process_tabs
            .read()
            .await
            .get(session_id)
            .map(|entry| entry.tab.clone())
    }

    /// Deliver `signal` to `session_id`'s process tab through its holder.
    pub async fn signal_process_tab(
        &self,
        session_id: &str,
        signal: i32,
    ) -> Result<(), SignalRefused> {
        let holder = self.process_tab_holder(session_id).await?;
        holder.signal(signal).await.map_err(SignalRefused::Holder)
    }

    /// Ask the holder of `session_id`'s process tab to stop its child, if the session has one. The
    /// tab stays until the holder reports the child's end.
    pub(super) async fn stop_process_tab(&self, session_id: &str) {
        let Ok(holder) = self.process_tab_holder(session_id).await else {
            return;
        };
        if let Err(error) = holder.stop().await {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "stop_session: session={session_id} — its process tab's holder could not stop it: {error}"
            );
        }
    }

    async fn process_tab_holder(
        &self,
        session_id: &str,
    ) -> Result<Arc<dyn ProcessTabHolder>, SignalRefused> {
        self.process_tabs
            .read()
            .await
            .get(session_id)
            .map(|entry| Arc::clone(&entry.holder))
            .ok_or(SignalRefused::NotHeld)
    }
}
