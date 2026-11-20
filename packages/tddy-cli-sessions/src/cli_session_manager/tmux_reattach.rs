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

/// The watchers of detached tmux-run agents, by session and terminal.
pub(crate) type DetachedWatchers = Arc<RwLock<Watchers>>;

/// The running watchers, and whether new ones may start: a manager that has shut down
/// ([`CliSessionManager::kill_all`]) starts none.
#[derive(Default)]
pub(crate) struct Watchers {
    closed: bool,
    /// `session_id → (terminal_id → watcher)`: a session's main and each of its tmux-wrapped tabs
    /// are watched on their own.
    by_session: HashMap<String, HashMap<String, tokio::task::AbortHandle>>,
    /// `(session_id, terminal_id)` of the tmux clients that just ended and whose agent has not yet
    /// been told apart from the client: held, so the session is never between its terminal and its
    /// watcher.
    settling: std::collections::HashSet<(String, String)>,
}

impl Watchers {
    /// Stop the watcher of `session_id`'s terminal `terminal_id`, if one runs.
    pub(crate) fn stop(&mut self, session_id: &str, terminal_id: &str) {
        let Some(terminals) = self.by_session.get_mut(session_id) else {
            return;
        };
        if let Some(watcher) = terminals.remove(terminal_id) {
            watcher.abort();
        }
        if terminals.is_empty() {
            self.by_session.remove(session_id);
        }
    }

    /// The sessions whose detached agent a watcher is still watching.
    pub(crate) fn watched_sessions(&self) -> Vec<String> {
        self.by_session
            .iter()
            .filter(|(_, terminals)| terminals.values().any(|watcher| !watcher.is_finished()))
            .map(|(session_id, _)| session_id.clone())
            .chain(
                self.settling
                    .iter()
                    .map(|(session_id, _)| session_id.clone()),
            )
            .collect()
    }

    /// Stop every watcher and refuse new ones.
    pub(crate) fn close(&mut self) {
        self.closed = true;
        for (_, terminals) in self.by_session.drain() {
            for (_, watcher) in terminals {
                watcher.abort();
            }
        }
    }
}

/// What a watcher of a detached agent reads and cleans up: the manager's shared state, cloned.
#[derive(Clone)]
pub(crate) struct DetachWatch {
    terminals: TerminalIndex,
    managed_workflows: ManagedWorkflowRegistry,
    exposed_terminals: ExposedTerminalRegistry,
    watchers: DetachedWatchers,
    /// The terminal this watch is for: the session's main unless [`Self::for_terminal`] says
    /// otherwise.
    terminal_id: String,
    interval: Duration,
    /// Where an agent end is announced — read by both announcing sites: a main ending with its
    /// agent (`main_ended`) and a detached agent whose tmux session is gone (`start`).
    agent_ends: super::agent_end::AgentEndSender,
}

impl DetachWatch {
    /// The same watch, for the terminal `terminal_id` of the session instead of its main. A tab's
    /// agent ending is not the session's agent ending, so a tab's watch drops no session state.
    pub(crate) fn for_terminal(mut self, terminal_id: &str) -> Self {
        self.terminal_id = terminal_id.to_string();
        self
    }

    /// Watch the tmux session `session` of `os_user`, which holds `session_id`'s agent, until it is
    /// gone or the terminal is registered for the session again. When it is gone a main's managed
    /// workflow (its toolcall socket) and exposure are dropped — the cleanup a main exiting with its
    /// agent runs. A watcher already running for the terminal is replaced. Nothing starts once the
    /// manager has shut down.
    pub(crate) async fn start(self, session_id: String, session: String, os_user: String) {
        let key = session_id.clone();
        let terminal_id = self.terminal_id.clone();
        let watchers = Arc::clone(&self.watchers);
        // Held across the spawn, so a concurrent `close` either sees this watcher or stops it first.
        let mut registry = watchers.write().await;
        if registry.closed {
            return;
        }
        registry.stop(&key, &terminal_id);
        let tracked = terminal_id.clone();
        let watcher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(self.interval).await;
                let reattached = self
                    .terminals
                    .read()
                    .await
                    .get(&session_id)
                    .is_some_and(|tools| tools.contains_key(&tracked));
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
                        // Only the main's agent is the session's: a tab's ending leaves the main's
                        // workflow and exposure, and announces nothing.
                        if tracked == MAIN_TERMINAL_ID {
                            self.drop_agent_state(&session_id).await;
                            // Nothing saw the agent's process exit, so its end has no code.
                            let _ = self.agent_ends.send(super::AgentEnd {
                                session_id: session_id.clone(),
                                os_user: Some(os_user.clone()),
                                outcome: "exit code unknown".to_string(),
                                exit_code: None,
                            });
                        }
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
        registry
            .by_session
            .entry(key)
            .or_default()
            .insert(terminal_id, watcher.abort_handle());
    }

    /// What a main terminal's end leaves of its session, by how it ended.
    ///
    /// The agent ending ends the session: its managed workflow is dropped so the per-session
    /// toolcall listener socket is cleaned up, and its exposure to LiveKit is forgotten — there is
    /// no terminal left to bridge, and an entry nobody removes would keep a dead session's block
    /// waiting for a consumer that could only find an empty PTY. A tmux client that ended while its
    /// agent lives keeps both, since its agent still serves them; nothing else will notice when that
    /// agent ends, so a watcher does, and runs the same cleanup then. An agent that ended is
    /// announced (`end`, built by the caller from the main's status) once the cleanup has run.
    pub(crate) async fn main_ended(
        self,
        session_id: String,
        exit: MainExit,
        end: Option<super::AgentEnd>,
    ) {
        match exit {
            MainExit::AgentEnded => {
                self.drop_agent_state(&session_id).await;
                if let Some(end) = end {
                    let _ = self.agent_ends.send(end);
                }
            }
            // The watcher took the session over before the terminal left the index
            // ([`Self::hold_detached_agent`]).
            MainExit::ClientOnly => {}
        }
    }

    /// Hold `session_id` from the moment its tmux client ended until [`Self::settled`]: whether the
    /// agent ended with its client is only known after a probe, and the session stays live until
    /// then.
    pub(crate) async fn settling(&self, session_id: &str) {
        log::debug!(
            target: "tddy_cli_sessions::reattach",
            "{session_id}: tmux client ended; holding the session while its agent is probed"
        );
        self.watchers
            .write()
            .await
            .settling
            .insert((session_id.to_string(), self.terminal_id.clone()));
    }

    /// Release the hold [`Self::settling`] took: the session is held by a watcher or not at all.
    pub(crate) async fn settled(&self, session_id: &str, main_exit: Option<MainExit>) {
        self.watchers
            .write()
            .await
            .settling
            .remove(&(session_id.to_string(), self.terminal_id.clone()));
        log::debug!(
            target: "tddy_cli_sessions::reattach",
            "{session_id}: settled as {main_exit:?}"
        );
    }

    /// Hand a session whose tmux client ended while its agent lives to a detach watcher. Called
    /// **before** the client's terminal leaves the index, so the session is held by the terminal or
    /// by the watcher at every moment — a listing never finds it held by neither.
    pub(crate) async fn hold_detached_agent(self, session_id: String, tmux_run: TmuxRun) {
        match (tmux_run.session, tmux_run.os_user) {
            (Some(session), Some(os_user)) => {
                log::debug!(
                    target: "tddy_cli_sessions::reattach",
                    "{session_id}: client ended, agent alive in tmux session {session}; watcher takes over"
                );
                self.start(session_id, session, os_user).await;
            }
            _ => log::debug!(
                target: "tddy_cli_sessions::reattach",
                "{session_id}: client ended but the run names no tmux session or user; no watcher"
            ),
        }
    }

    /// The cleanup of a session whose agent ended: its managed workflow and its exposure.
    async fn drop_agent_state(&self, session_id: &str) {
        self.managed_workflows.write().await.remove(session_id);
        self.exposed_terminals.write().await.remove(session_id);
    }
}

/// The tmux session a terminal is a client of, and the OS user it runs as — both `None` for a
/// terminal that is not a tmux client.
#[derive(Clone)]
pub(crate) struct TmuxRun {
    session: Option<String>,
    os_user: Option<String>,
}

impl TmuxRun {
    pub(crate) fn new(session: Option<&str>, os_user: Option<&str>) -> Self {
        TmuxRun {
            session: session.map(str::to_string),
            os_user: os_user.map(str::to_string),
        }
    }

    /// The OS user the terminal ran as; `None` when it ran as the daemon's own user.
    pub(crate) fn os_user(&self) -> Option<&str> {
        self.os_user.as_deref()
    }

    /// Whether the terminal is a client of a tmux session (a tmux-run main), rather than the
    /// agent's own process (a PTY-run main).
    pub(crate) fn is_tmux_run(&self) -> bool {
        self.session.is_some()
    }
}

/// How a main terminal's process ended. The liveness probe runs a process, so it runs on the
/// blocking pool. A probe that did not finish keeps the session's state: nothing is torn down on a
/// guess.
pub(crate) async fn classify_ended_main(tmux_run: TmuxRun) -> MainExit {
    let TmuxRun { session, os_user } = tmux_run;
    let classified = tokio::task::spawn_blocking(move || {
        classify_main_exit(session.as_deref(), |name| {
            os_user
                .as_deref()
                .is_some_and(|user| tmux_session_alive(user, name))
        })
    })
    .await;
    match classified {
        Ok(exit) => exit,
        Err(error) => {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not tell whether a main terminal's agent is still running, so its session is kept: {error}"
            );
            MainExit::ClientOnly
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
            terminal_id: MAIN_TERMINAL_ID.to_string(),
            interval: self.detach_watch_interval,
            agent_ends: self.agent_end_tx.clone(),
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
        self.reattach_terminal(session_id, MAIN_TERMINAL_ID, kind, worktree, os_user, name)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_manager_that_shut_down_starts_no_watcher_for_a_detached_agent() {
        // Given a manager that has shut down
        let manager = CliSessionManager::new();
        manager.kill_all().await;

        // When a main cancelled by that shutdown exits as a client only
        manager
            .detach_watch()
            .start("session-1".into(), "tddy-session".into(), "user".into())
            .await;

        // Then no watcher outlives the shutdown
        assert!(manager.detached_watchers.read().await.by_session.is_empty());
    }

    #[tokio::test]
    async fn a_running_manager_registers_a_watcher_for_a_detached_agent() {
        // Given a running manager
        let manager = CliSessionManager::new();

        // When a tmux-run main exits as a client only
        manager
            .detach_watch()
            .start("session-1".into(), "tddy-session".into(), "user".into())
            .await;

        // Then a watcher is registered for its session
        assert!(manager
            .detached_watchers
            .read()
            .await
            .by_session
            .contains_key("session-1"));
    }
}
