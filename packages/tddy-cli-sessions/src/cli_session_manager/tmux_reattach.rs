//! Re-attaching a session's main terminal to the tmux session its agent runs in, and telling a
//! tmux client ending from the agent ending (`#tmux-agent` 2/3).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use super::{
    pty_handle, CliSessionManager, ExposedTerminalRegistry, ManagedWorkflowRegistry, TerminalIndex,
    MAIN_TERMINAL_ID,
};
use crate::tmux::tmux_session_alive;

/// How often a detached tmux-run agent is checked for being gone.
pub const DETACH_WATCH_INTERVAL: Duration = Duration::from_secs(10);

/// The watchers of detached tmux-run agents, by session.
pub(crate) type DetachedWatchers = Arc<RwLock<HashMap<String, tokio::task::AbortHandle>>>;

/// What a watcher of a detached agent reads and cleans up: the manager's shared state, cloned.
pub(crate) struct DetachWatch {
    terminals: TerminalIndex,
    managed_workflows: ManagedWorkflowRegistry,
    exposed_terminals: ExposedTerminalRegistry,
    watchers: DetachedWatchers,
    interval: Duration,
}

impl DetachWatch {
    /// Watch the tmux session `session` of `os_user`, which holds `session_id`'s agent, until it is
    /// gone or a main terminal is registered for the session again. When it is gone the session's
    /// managed workflow (its toolcall socket) and exposure are dropped — the cleanup a main exiting
    /// with its agent runs. A watcher already running for the session is replaced.
    pub(crate) async fn start(self, session_id: String, session: String, os_user: String) {
        let key = session_id.clone();
        let watchers = Arc::clone(&self.watchers);
        let watcher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(self.interval).await;
                let reattached = self
                    .terminals
                    .read()
                    .await
                    .get(&session_id)
                    .is_some_and(|tools| tools.contains_key(MAIN_TERMINAL_ID));
                if reattached {
                    return;
                }
                let alive = {
                    let (os_user, session) = (os_user.clone(), session.clone());
                    tokio::task::spawn_blocking(move || tmux_session_alive(&os_user, &session))
                        .await
                };
                match alive {
                    Ok(true) => continue,
                    Ok(false) => {
                        log::info!(
                            target: "tddy_daemon::claude_cli_session",
                            "the agent of detached session {session_id} ended (tmux session {session:?} is gone): cleaning up"
                        );
                        self.managed_workflows.write().await.remove(&session_id);
                        self.exposed_terminals.write().await.remove(&session_id);
                        return;
                    }
                    // A probe that did not finish tears nothing down: it is tried again.
                    Err(error) => log::warn!(
                        target: "tddy_daemon::claude_cli_session",
                        "could not tell whether tmux session {session:?} is alive: {error}"
                    ),
                }
            }
        });
        let replaced = watchers.write().await.insert(key, watcher.abort_handle());
        if let Some(replaced) = replaced {
            replaced.abort();
        }
    }
}

/// Why a main terminal's process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainExit {
    /// The tmux *client* ended (a detach, a shutdown) and the agent's tmux session is alive: the
    /// terminal entry is dropped, but the session's workflow and exposure stay.
    ClientOnly,
    /// The agent ended — a PTY-run main, or a tmux-run main whose tmux session is gone: today's
    /// main-exit cleanup.
    AgentEnded,
}

/// Classify the end of a main terminal. `tmux_session` is the terminal's recorded tmux session
/// (`None` for a PTY-run main); `tmux_session_alive` is the liveness probe, injected so the
/// decision is testable without a tmux server. A terminal without a tmux session never probes.
pub fn classify_main_exit(
    tmux_session: Option<&str>,
    tmux_session_alive: impl FnOnce(&str) -> bool,
) -> MainExit {
    match tmux_session {
        Some(name) if tmux_session_alive(name) => MainExit::ClientOnly,
        _ => MainExit::AgentEnded,
    }
}

impl CliSessionManager {
    /// The same manager, checking a detached tmux-run agent for being gone every `interval` instead
    /// of [`DETACH_WATCH_INTERVAL`].
    #[must_use]
    pub fn with_detach_watch_interval(mut self, interval: Duration) -> Self {
        self.detach_watch_interval = interval;
        self
    }

    pub(crate) fn detach_watch(&self) -> DetachWatch {
        DetachWatch {
            terminals: Arc::clone(&self.terminals),
            managed_workflows: Arc::clone(&self.managed_workflows),
            exposed_terminals: Arc::clone(&self.exposed_terminals),
            watchers: Arc::clone(&self.detached_watchers),
            interval: self.detach_watch_interval,
        }
    }

    /// Whether `session_id` has a managed workflow registered with this manager: its toolcall
    /// socket is bound by this daemon.
    pub async fn has_managed_workflow(&self, session_id: &str) -> bool {
        self.managed_workflows.read().await.contains_key(session_id)
    }

    /// Re-attach `session_id`'s main terminal to the live tmux session `name`: what
    /// [`attach_main_to_tmux`](Self::attach_main_to_tmux) does. Returns the main handle.
    ///
    /// A main already attached to `name` is returned as it is, so a second resume of a live session
    /// does not add a second client. The agent is not touched.
    ///
    /// The caller registers the main recording, as it does after a start (the recording path is the
    /// caller's to name, not this manager's). The exposure record is not re-registered here: a
    /// client's exit in this daemon keeps it (see [`classify_main_exit`]), and a restarted daemon has
    /// no record to re-register from.
    pub async fn reattach_main(
        &self,
        session_id: &str,
        os_user: &str,
        name: &str,
        worktree: PathBuf,
        kind: &str,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        if let Some(attached) = self
            .get(session_id)
            .await
            .filter(|main| main.tmux_session.as_deref() == Some(name))
        {
            return Ok(attached);
        }
        if let Some(watcher) = self.detached_watchers.write().await.remove(session_id) {
            watcher.abort();
        }
        self.attach_main_to_tmux(session_id, worktree, os_user, kind, name)
            .await
    }
}
