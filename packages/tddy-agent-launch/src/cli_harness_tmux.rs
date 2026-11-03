//! Running cursor-cli and the registered CLI harnesses (opencode, muse, warp) inside tmux
//! (`#tmux-agent` 3/3, docs/ft/daemon/1-WIP/PRD-2026-11-03-harnesses-in-tmux.md).
//!
//! The same shape as [`crate::claude_cli_tmux`] — the agent runs in a tmux session of its own as
//! the session's OS user, the main terminal is a client attached to it, the pid recorded is the
//! pane's — for a harness whose argv the caller has already built (the registry's, plus a route's
//! arguments; cursor's chat-id argv). A harness with an exit hook runs it on the *agent's* exit
//! with the pane's final screen (`remain-on-exit`), never on a client's.

use std::path::PathBuf;

use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_cli_sessions::tmux::{
    agent_tmux_session_name, kill_tmux_session, pane_pid, pane_state, start_agent_tmux_session,
    PaneState,
};
use tddy_rpc::Status;

pub use crate::claude_cli_tmux::TmuxLaunched;
use crate::claude_cli_tmux::{in_tmux_blocking, stop_tmux_session_of_failed_start};

/// What a tmux-run harness is launched with.
pub struct HarnessTmuxLaunch<'a> {
    pub os_user: &'a str,
    pub session_id: &'a str,
    pub worktree_path: PathBuf,
    /// The session's directory, which holds the pane's environment file.
    pub session_dir: PathBuf,
    /// The main terminal's kind: the session type (`cursor-cli`, `warp-cli`, ...).
    pub kind: &'a str,
    /// The harness's complete argv, binary first.
    pub argv: Vec<String>,
    /// Set on the tmux *pane*, through a `0600` file under `session_dir` the pane sources and
    /// deletes (`tddy_cli_sessions::tmux::pane_env`): a pane inherits the tmux server's
    /// environment, not the client's, and no value may go on an argv.
    pub env: Vec<(String, String)>,
    /// Run when the agent exits, with the pane's final screen. Its presence sets `remain-on-exit`.
    pub on_exit: Option<ExitHook>,
}

/// Start the harness in a new tmux session for `launch.session_id` and attach the session's main
/// terminal to it; with `on_exit`, the hook source is registered
/// ([`CliSessionManager::run_exit_hook_on_agent_exit`]).
pub async fn start_harness_in_tmux(
    manager: &CliSessionManager,
    launch: HarnessTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    let HarnessTmuxLaunch {
        os_user,
        session_id,
        worktree_path,
        session_dir,
        kind,
        argv,
        env,
        on_exit,
    } = launch;
    let name = agent_tmux_session_name(session_id);
    let remain_on_exit = on_exit.is_some();
    let pane_pid = {
        let (os_user_owned, name_owned, worktree) =
            (os_user.to_string(), name.clone(), worktree_path.clone());
        in_tmux_blocking(move || {
            start_agent_tmux_session(
                &os_user_owned,
                &name_owned,
                &worktree,
                &session_dir,
                &env,
                &argv,
                remain_on_exit,
            )
        })
        .await?
        .map_err(|e| Status::internal(format!("failed to spawn {kind}: {e}")))?
    };
    let handle = match manager
        .attach_main_to_tmux(session_id, worktree_path, os_user, kind, &name)
        .await
    {
        Ok(handle) => handle,
        Err(error) => {
            // The agent must not outlive a start that failed: nothing records it, so nothing could
            // stop it later.
            stop_tmux_session_of_failed_start(os_user, &name).await;
            return Err(Status::internal(format!(
                "failed to attach the main terminal to tmux session {name:?}: {error}"
            )));
        }
    };
    if let Some(on_exit) = on_exit {
        manager.run_exit_hook_on_agent_exit(session_id, os_user, &name, on_exit);
    }
    Ok(TmuxLaunched {
        handle,
        pane_pid,
        tmux_session: name,
    })
}

/// Resume a tmux-run harness session: re-attach to its tmux session while the agent runs
/// (`CliSessionManager::reattach_main`), else start the harness afresh with the harness's resume
/// argv (`launch.argv`). A pane whose agent has exited is not re-attached: no daemon watched it end,
/// so it is ended and the resume starts afresh.
pub async fn resume_harness_in_tmux(
    manager: &CliSessionManager,
    launch: HarnessTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    let name = agent_tmux_session_name(launch.session_id);
    let state = {
        let (probe_user, probe_name) = (launch.os_user.to_string(), name.clone());
        in_tmux_blocking(move || pane_state(&probe_user, &probe_name))
            .await?
            .map_err(|e| {
                Status::internal(format!(
                    "could not read the pane of tmux session {name:?}: {e}"
                ))
            })?
    };
    match state {
        PaneState::Running => reattach_running_harness(manager, launch, name).await,
        PaneState::Exited | PaneState::SessionGone => {
            if state == PaneState::Exited {
                log::warn!(
                    target: "tddy_daemon::claude_cli_session",
                    "the agent of {} exited while no daemon watched tmux session {name:?}: its exit hook did not run, and the resume starts afresh",
                    launch.session_id
                );
            }
            let (end_user, end_name) = (launch.os_user.to_string(), name.clone());
            in_tmux_blocking(move || kill_tmux_session(&end_user, &end_name))
                .await?
                .map_err(|e| {
                    Status::internal(format!(
                        "could not clear the leftover tmux session {name:?}: {e}"
                    ))
                })?;
            start_harness_in_tmux(manager, launch).await
        }
    }
}

/// Re-attach the main terminal to the running agent of tmux session `name`, re-record its pane's
/// pid, and register the exit hook again: a daemon restart lost the watcher that held it.
async fn reattach_running_harness(
    manager: &CliSessionManager,
    launch: HarnessTmuxLaunch<'_>,
    name: String,
) -> Result<TmuxLaunched, Status> {
    let HarnessTmuxLaunch {
        os_user,
        session_id,
        worktree_path,
        kind,
        on_exit,
        ..
    } = launch;
    // The pane's pid is read before a client is attached, so a failure to read it leaves nothing
    // attached behind.
    let pid = {
        let (probe_user, probe_name) = (os_user.to_string(), name.clone());
        in_tmux_blocking(move || pane_pid(&probe_user, &probe_name))
            .await?
            .map_err(|e| {
                Status::internal(format!(
                    "could not read the pane of tmux session {name:?}: {e}"
                ))
            })?
    };
    let handle = manager
        .reattach_main(session_id, os_user, &name, worktree_path, kind)
        .await
        .map_err(|e| {
            Status::internal(format!(
                "failed to re-attach the main terminal to tmux session {name:?}: {e}"
            ))
        })?;
    if let Some(on_exit) = on_exit {
        manager.run_exit_hook_on_agent_exit(session_id, os_user, &name, on_exit);
    }
    Ok(TmuxLaunched {
        handle,
        pane_pid: pid,
        tmux_session: name,
    })
}
