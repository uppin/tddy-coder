//! A **process tab**: a session tab with no PTY — a tool session's `tddy-coder`, held by a spawn
//! backend rather than by this manager (`#session-liveness` 3/4).
//!
//! It is the session's `main` tab, so code asking whether main is served keeps working for tool
//! sessions. It counts as a live agent ([`CliSessionManager::live_agent_sessions`]), it is not
//! listed by [`CliSessionManager::list_terminals`] (it has no terminal to stream), and it leaves the
//! registry when its [`ExitReceiver`] resolves. Stopping and signalling it go to its
//! [`ProcessTabHolder`] — the holder owns the child, so the pid is valid until the holder reports the
//! exit, which the pid a session's `.session.yaml` records never is.

use std::sync::Arc;

use tddy_task::ExitReceiver;

use super::CliSessionManager;

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

impl CliSessionManager {
    /// Hold `session_id`'s child `pid` as its `main` tab of `kind`, until `exit` resolves. `holder`
    /// is what stops and signals it.
    pub async fn register_process_tab(
        &self,
        session_id: &str,
        kind: &str,
        pid: u32,
        exit: ExitReceiver,
        holder: Arc<dyn ProcessTabHolder>,
    ) -> anyhow::Result<()> {
        // TODO(session-liveness 3/4): implemented in the green step.
        let _ = (session_id, kind, pid, exit, holder);
        todo!("hold the process tab under main until its exit resolves")
    }

    /// The process tab `session_id` holds, if any.
    pub async fn process_tab(&self, session_id: &str) -> Option<ProcessTab> {
        // TODO(session-liveness 3/4): implemented in the green step.
        let _ = session_id;
        todo!("look up the session's process tab")
    }

    /// Deliver `signal` to `session_id`'s process tab through its holder.
    pub async fn signal_process_tab(
        &self,
        session_id: &str,
        signal: i32,
    ) -> Result<(), SignalRefused> {
        // TODO(session-liveness 3/4): implemented in the green step.
        let _ = (session_id, signal);
        todo!("signal the process tab through its holder")
    }
}
