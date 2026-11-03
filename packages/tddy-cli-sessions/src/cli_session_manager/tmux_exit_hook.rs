//! The exit-hook source of a tmux-run main (`#tmux-agent` 3/3): a hook runs when the *agent*
//! exits, with the pane's final screen as its output — never when only a tmux client ended.
//!
//! A tmux-run agent with an exit hook keeps its pane after the process exits (`remain-on-exit`), so
//! its final screen survives for the hook to read. Nothing else tells the agent's end from a
//! client's, so a watcher polls the pane until it has exited; then the hook runs, and the tmux
//! session is ended. The watcher ends when the session is gone without an exited pane (killed by
//! someone else), and runs nothing then.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tddy_task::{TaskId, TerminalCapture};
use tokio::sync::watch;

use super::tmux_reattach::MainExit;
use super::{CliSessionManager, ExitHook, ExitSettlements, MAIN_TERMINAL_ID};
use crate::tmux::{capture_final_screen, kill_tmux_session, pane_state, PaneState, TmuxError};

/// How often a tmux-run agent's pane is checked for having exited.
pub(crate) const EXIT_HOOK_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// The sessions whose agent already has a watcher: a second registration for one of them is
/// dropped, because the watcher holds the hook of the run that is still going.
pub(super) type ExitHookWatched = Arc<Mutex<HashSet<String>>>;

/// The output an exit hook is handed for a tmux-run main that ended as `exit`: the final screen
/// as a capture when the agent ended, `None` — and `final_screen` never called — when only the
/// client did. A final screen that cannot be read is logged and yields `None`, so the hook does not
/// run on an empty screen.
pub fn exit_hook_capture(
    exit: MainExit,
    final_screen: impl FnOnce() -> Result<String, TmuxError>,
) -> Option<Arc<Mutex<TerminalCapture>>> {
    match exit {
        MainExit::ClientOnly => None,
        MainExit::AgentEnded => match final_screen() {
            Ok(screen) => {
                let mut capture = TerminalCapture::new();
                capture.append(screen.as_bytes());
                Some(Arc::new(Mutex::new(capture)))
            }
            Err(error) => {
                log::error!(
                    target: "tddy_daemon::claude_cli_session",
                    "the final screen of a tmux-run agent could not be read, so its exit hook does not run: {error}"
                );
                None
            }
        },
    }
}

/// What a watcher of one tmux-run agent needs: the manager's exit settlements and watched set.
struct ExitHookWatch {
    settlements: ExitSettlements,
    watched: ExitHookWatched,
}

impl CliSessionManager {
    /// Run `on_exit` once the agent of `session_id`'s tmux session `name` has exited, with the
    /// pane's final screen (`remain-on-exit`), then end the tmux session. A client exit never runs
    /// it. While the hook runs, it is the session's pending exit, so [`Self::exit_hook_settled`]
    /// waits on it.
    ///
    /// Called again for a session whose agent is already watched (a second resume of a live
    /// session), the new hook is dropped: the watcher holds the hook of the run that is going.
    pub fn run_exit_hook_on_agent_exit(
        &self,
        session_id: &str,
        os_user: &str,
        name: &str,
        on_exit: ExitHook,
    ) {
        let newly_watched = self
            .exit_hook_watched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id.to_string());
        if !newly_watched {
            return;
        }
        let watch = ExitHookWatch {
            settlements: Arc::clone(&self.exit_settlements),
            watched: Arc::clone(&self.exit_hook_watched),
        };
        let (session_id, os_user, name) = (
            session_id.to_string(),
            os_user.to_string(),
            name.to_string(),
        );
        tokio::spawn(async move {
            watch.run(session_id, os_user, name, on_exit).await;
        });
    }
}

impl ExitHookWatch {
    async fn run(self, session_id: String, os_user: String, name: String, on_exit: ExitHook) {
        if wait_for_agent_exit(&os_user, &name).await {
            self.run_hook(&session_id, &os_user, &name, on_exit).await;
        }
        self.watched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&session_id);
    }

    /// The agent has exited: its hook runs on the final screen while the session is the pending
    /// exit, then the tmux session is ended, which ends the client attached to it.
    async fn run_hook(&self, session_id: &str, os_user: &str, name: &str, on_exit: ExitHook) {
        let (settled, settled_rx) = watch::channel(false);
        let id = TaskId::new();
        let key = (session_id.to_string(), MAIN_TERMINAL_ID.to_string());
        self.settlements
            .write()
            .await
            .insert(key.clone(), (id.clone(), settled_rx));

        let (read_user, read_name) = (os_user.to_string(), name.to_string());
        let capture = tokio::task::spawn_blocking(move || {
            exit_hook_capture(MainExit::AgentEnded, || {
                capture_final_screen(&read_user, &read_name)
            })
        })
        .await;
        match capture {
            Ok(Some(capture)) => on_exit(capture).await,
            Ok(None) => {}
            Err(error) => log::error!(
                target: "tddy_daemon::claude_cli_session",
                "the final screen of tmux session {name:?} was not read: {error}"
            ),
        }
        let _ = settled.send(true);
        {
            let mut pending = self.settlements.write().await;
            if pending
                .get(&key)
                .is_some_and(|(pending_id, _)| *pending_id == id)
            {
                pending.remove(&key);
            }
        }

        let (end_user, end_name) = (os_user.to_string(), name.to_string());
        let ended =
            tokio::task::spawn_blocking(move || kill_tmux_session(&end_user, &end_name)).await;
        match ended {
            Ok(Ok(())) => {}
            Ok(Err(error)) => log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not end tmux session {name:?} of session {session_id} after its agent exited: {error}"
            ),
            Err(error) => log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "ending tmux session {name:?} of session {session_id} did not finish: {error}"
            ),
        }
    }
}

/// Polls the pane of tmux session `name` until its agent has exited (`true`), or the session is
/// gone without one (`false`). A probe that fails is logged and tried again.
async fn wait_for_agent_exit(os_user: &str, name: &str) -> bool {
    loop {
        let (probe_user, probe_name) = (os_user.to_string(), name.to_string());
        let state = tokio::task::spawn_blocking(move || pane_state(&probe_user, &probe_name)).await;
        match state {
            Ok(Ok(PaneState::Running)) => {}
            Ok(Ok(PaneState::Exited)) => return true,
            Ok(Ok(PaneState::SessionGone)) => {
                log::info!(
                    target: "tddy_daemon::claude_cli_session",
                    "tmux session {name:?} is gone before its agent exited: no exit hook runs"
                );
                return false;
            }
            Ok(Err(error)) => log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not read the pane of tmux session {name:?}, checking again: {error}"
            ),
            Err(error) => log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "the pane check of tmux session {name:?} did not finish, checking again: {error}"
            ),
        }
        tokio::time::sleep(EXIT_HOOK_POLL_INTERVAL).await;
    }
}
