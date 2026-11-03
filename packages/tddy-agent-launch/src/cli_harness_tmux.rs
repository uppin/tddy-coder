//! Running cursor-cli and the registered CLI harnesses (opencode, muse, warp) inside tmux
//! (`#tmux-agent` 3/3, docs/ft/daemon/1-WIP/PRD-2026-11-03-harnesses-in-tmux.md).
//!
//! The same shape as [`crate::claude_cli_tmux`] — the agent runs in a tmux session of its own as
//! the session's OS user, the main terminal is a client attached to it, the pid recorded is the
//! pane's — for a harness whose argv the caller has already built (the registry's, plus a route's
//! arguments; cursor's chat-id argv). A harness with an exit hook runs it on the *agent's* exit
//! with the pane's final screen (`remain-on-exit`), never on a client's.
//!
//! After a daemon restart the main terminal is re-attached lazily, on the first stream
//! ([`reattach_recorded_harness`]), and the exit hook is watched again from then on.

use std::path::{Path, PathBuf};

use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_cli_sessions::tmux::{
    agent_tmux_session_name, kill_tmux_session, pane_pid, pane_state, start_agent_tmux_session,
    PaneState,
};
use tddy_rpc::Status;

pub use crate::claude_cli_tmux::TmuxLaunched;
use crate::claude_cli_tmux::{in_tmux_blocking, stop_tmux_session_of_failed_start};
use crate::cli_resume_token::resume_token_recorder;

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
        PaneState::Running => {
            let running = RunningHarness {
                os_user: launch.os_user,
                session_id: launch.session_id,
                worktree_path: launch.worktree_path,
                kind: launch.kind,
            };
            reattach_running_harness(manager, running, launch.on_exit, name).await
        }
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

/// A tmux-run harness session whose agent is running, for a re-attach.
struct RunningHarness<'a> {
    os_user: &'a str,
    session_id: &'a str,
    worktree_path: PathBuf,
    /// The main terminal's kind: the session type.
    kind: &'a str,
}

/// Re-attach the main terminal to the running agent of tmux session `name`, re-record its pane's
/// pid, and register the exit hook again: a daemon restart lost the watcher that held it.
async fn reattach_running_harness(
    manager: &CliSessionManager,
    running: RunningHarness<'_>,
    on_exit: Option<ExitHook>,
    name: String,
) -> Result<TmuxLaunched, Status> {
    let RunningHarness {
        os_user,
        session_id,
        worktree_path,
        kind,
    } = running;
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

/// A tmux-run harness session (cursor-cli, opencode-cli, muse-cli, warp-cli) as its
/// `.session.yaml` records it, for the lazy re-attach of its main terminal.
pub struct RecordedHarness<'a> {
    pub os_user: &'a str,
    pub session_id: &'a str,
    /// The session type the session recorded, which is the main terminal's kind.
    pub session_type: &'a str,
    pub worktree_path: PathBuf,
    pub session_dir: &'a Path,
}

/// Re-attach the main terminal of a recorded tmux-run harness session whose daemon or client went
/// away — the agent is not relaunched — and watch its agent's exit again, so its exit hook (Warp's
/// resume token) runs even when the agent exits with no client attached. `true` when a main is
/// attached; `false` when there is none to attach: the tmux session is gone, or its agent exited
/// while no daemon watched it, in which case the exit hook runs now on the pane's final screen and
/// ends the tmux session.
pub async fn reattach_recorded_harness(
    manager: &CliSessionManager,
    recorded: RecordedHarness<'_>,
) -> Result<bool, Status> {
    let RecordedHarness {
        os_user,
        session_id,
        session_type,
        worktree_path,
        session_dir,
    } = recorded;
    let name = agent_tmux_session_name(session_id);
    let state = {
        let (probe_user, probe_name) = (os_user.to_string(), name.clone());
        in_tmux_blocking(move || pane_state(&probe_user, &probe_name))
            .await?
            .map_err(|e| {
                Status::internal(format!(
                    "could not read the pane of tmux session {name:?}: {e}"
                ))
            })?
    };
    let on_exit = recorded_exit_hook(session_type, session_dir);
    match (state, on_exit) {
        (PaneState::Running, on_exit) => {
            let running = RunningHarness {
                os_user,
                session_id,
                worktree_path,
                kind: session_type,
            };
            reattach_running_harness(manager, running, on_exit, name).await?;
            Ok(true)
        }
        (PaneState::Exited, Some(on_exit)) => {
            log::info!(
                target: "tddy_daemon::claude_cli_session",
                "the agent of {session_id} exited while no daemon watched tmux session {name:?}: running its exit hook on the final screen"
            );
            manager.run_exit_hook_on_agent_exit(session_id, os_user, &name, on_exit);
            Ok(false)
        }
        // A pane kept without a hook to read it holds nothing anyone needs.
        (PaneState::Exited, None) => {
            let (end_user, end_name) = (os_user.to_string(), name.clone());
            in_tmux_blocking(move || kill_tmux_session(&end_user, &end_name))
                .await?
                .map_err(|e| {
                    Status::internal(format!("could not end tmux session {name:?}: {e}"))
                })?;
            Ok(false)
        }
        (PaneState::SessionGone, _) => Ok(false),
    }
}

/// The exit hook a re-attached harness is watched with — the one its launch registered. Every
/// harness the generic launcher runs (opencode, muse, warp) records the resume token its run
/// prints as it exits; claude-cli and cursor-cli register none. The session's metadata is already
/// on disk, so the recorder is armed at once.
fn recorded_exit_hook(session_type: &str, session_dir: &Path) -> Option<ExitHook> {
    let generic = tddy_core::cli_harness::cli_harness(session_type).is_some()
        && !matches!(session_type, "claude-cli" | "cursor-cli");
    if !generic {
        return None;
    }
    let (hook, arm) = resume_token_recorder(session_dir.to_path_buf());
    arm.arm();
    Some(hook)
}
