//! Running a Claude CLI agent inside tmux (`#tmux-agent` 1/3,
//! docs/ft/daemon/claude-cli-session.md § tmux mode).
//!
//! The agent runs in a tmux session of its own, as the session's OS user; the session's main
//! terminal is a tmux client attached to it, and the pid the session records is the pane's process —
//! so liveness, signal and delete act on the agent, not on the client.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::tmux::{
    agent_tmux_session_name, kill_tmux_session, pane_pid, start_agent_tmux_session,
    tmux_session_alive,
};
use tddy_rpc::Status;

/// What a tmux-run claude is launched with: the arguments [`CliSessionManager::start_with_options`]
/// takes for a PTY-child launch, so the two launches build the same `claude` argv. The default is
/// no prompt, the default permission mode and a fresh (not resumed) session.
#[derive(Default)]
pub struct ClaudeCliTmuxLaunch<'a> {
    pub os_user: &'a str,
    pub session_id: &'a str,
    pub worktree_path: PathBuf,
    pub model: &'a str,
    pub binary: &'a str,
    pub initial_prompt: Option<&'a str>,
    pub permission_mode: Option<&'a str>,
    pub dangerously_skip_permissions: bool,
    /// Continue the on-disk transcript (`--resume <id>`) instead of assigning the session id.
    pub resume: bool,
    pub append_system_prompt_file: Option<&'a Path>,
    pub extra_args: Vec<String>,
    /// Set on the tmux *pane*, through a `0600` file under `session_dir` the pane sources and
    /// deletes (`tddy_cli_sessions::tmux::pane_env`): a pane inherits the tmux server's
    /// environment, not the client's, and no value may go on an argv.
    pub env: Vec<(String, String)>,
    /// The session's directory (absolute), which holds the pane environment file. Unused by a
    /// PTY-child launch.
    pub session_dir: PathBuf,
}

/// A claude agent running in tmux, and the main terminal attached to it.
pub struct TmuxLaunched {
    /// The main terminal: a tmux client of [`Self::tmux_session`].
    pub handle: Arc<PtyHandle>,
    /// The pid of the agent's pane process — what the session records as its `pid`.
    pub pane_pid: u32,
    pub tmux_session: String,
}

/// Start claude in a new tmux session for `launch.session_id`, then attach the session's main
/// terminal to it.
///
/// The argv is the one a PTY-hosted start builds ([`CliSessionManager::build_claude_argv`] with
/// [`CliSessionManager::with_claude_launch_flags`]).
/// The pane is started with `launch.env` on it (from a file, never an argv), and its pid is the
/// agent's.
pub async fn start_claude_in_tmux(
    manager: &CliSessionManager,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    let ClaudeCliTmuxLaunch {
        os_user,
        session_id,
        worktree_path,
        model,
        binary,
        initial_prompt,
        permission_mode,
        dangerously_skip_permissions,
        resume,
        append_system_prompt_file,
        extra_args,
        env,
        session_dir,
    } = launch;
    let name = agent_tmux_session_name(session_id);
    let argv = CliSessionManager::with_claude_launch_flags(
        CliSessionManager::build_claude_argv(
            binary,
            model,
            session_id,
            initial_prompt,
            permission_mode,
            dangerously_skip_permissions,
            resume,
        ),
        initial_prompt,
        append_system_prompt_file,
        extra_args,
    );
    let pane_pid = start_agent_pane(os_user, &name, &worktree_path, session_dir, env, argv).await?;
    let handle = match manager
        .attach_main_to_tmux(session_id, worktree_path, os_user, "claude-cli", &name)
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
    Ok(TmuxLaunched {
        handle,
        pane_pid,
        tmux_session: name,
    })
}

/// Start `argv` in the agent's tmux session `name`, off the executor, and return the pane's pid.
async fn start_agent_pane(
    os_user: &str,
    name: &str,
    worktree: &Path,
    session_dir: PathBuf,
    env: Vec<(String, String)>,
    argv: Vec<String>,
) -> Result<u32, Status> {
    let (os_user, name, worktree) = (
        os_user.to_string(),
        name.to_string(),
        worktree.to_path_buf(),
    );
    in_tmux_blocking(move || {
        // Claude has no exit hook, so its pane is not kept after the agent exits.
        start_agent_tmux_session(&os_user, &name, &worktree, &session_dir, &env, &argv, false)
    })
    .await?
    .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {e}")))
}

/// Resume a tmux-run session: kill the leftover tmux session of the terminated run, then start
/// claude in a fresh one with the resume argv. The resume argv is always used, whatever the caller
/// set in `launch.resume`.
pub async fn resume_claude_in_tmux(
    manager: &CliSessionManager,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    // The terminated run may have left its tmux session behind (its client gone, its agent not),
    // and the fresh session needs the name. Killing a session that is not there is not an error.
    let name = agent_tmux_session_name(launch.session_id);
    let os_user = launch.os_user.to_string();
    in_tmux_blocking({
        let name = name.clone();
        move || kill_tmux_session(&os_user, &name)
    })
    .await?
    .map_err(|e| {
        Status::internal(format!(
            "could not clear the leftover tmux session {name:?}: {e}"
        ))
    })?;
    start_claude_in_tmux(
        manager,
        ClaudeCliTmuxLaunch {
            resume: true,
            ..launch
        },
    )
    .await
}

/// Run a blocking tmux operation off the async executor. Every tmux call spawns a process and
/// waits for it, so none of them may run on the executor's threads.
pub(crate) async fn in_tmux_blocking<T, F>(operation: F) -> Result<T, Status>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|e| Status::internal(format!("tmux task did not finish: {e}")))
}

/// Kill the tmux session of a start that failed after it began. The start's own error is what the
/// caller is told, so a kill that fails is logged rather than returned.
pub(crate) async fn stop_tmux_session_of_failed_start(os_user: &str, name: &str) {
    let (os_user, name) = (os_user.to_string(), name.to_string());
    let outcome = tokio::task::spawn_blocking({
        let name = name.clone();
        move || kill_tmux_session(&os_user, &name)
    })
    .await;
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::warn!(
            target: "tddy_daemon::claude_cli_session",
            "could not stop tmux session {name:?} of a start that failed: {error}"
        ),
        Err(error) => log::warn!(
            target: "tddy_daemon::claude_cli_session",
            "the stop of tmux session {name:?} of a start that failed did not finish: {error}"
        ),
    }
}

/// Resume a tmux-run session whose tmux session is still alive: re-attach the main terminal to it
/// (`CliSessionManager::reattach_main`) and re-record the pane's pid — the agent is not
/// relaunched. `launch.resume` is true.
pub async fn reattach_claude_in_tmux(
    manager: &CliSessionManager,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    let name = agent_tmux_session_name(launch.session_id);
    let os_user = launch.os_user.to_string();
    // The pane's pid is read before a client is attached, so a failure to read it leaves nothing
    // attached behind.
    let pid = {
        let (probe_user, probe_name) = (os_user.clone(), name.clone());
        in_tmux_blocking(move || pane_pid(&probe_user, &probe_name))
            .await?
            .map_err(|e| {
                Status::internal(format!(
                    "could not read the pane of tmux session {name:?}: {e}"
                ))
            })?
    };
    let handle = manager
        .reattach_main(
            launch.session_id,
            &os_user,
            &name,
            launch.worktree_path,
            "claude-cli",
        )
        .await
        .map_err(|e| {
            Status::internal(format!(
                "failed to re-attach the main terminal to tmux session {name:?}: {e}"
            ))
        })?;
    Ok(TmuxLaunched {
        handle,
        pane_pid: pid,
        tmux_session: name,
    })
}

/// Whether the tmux session of `session_id`'s agent is alive for `os_user`, probed on the blocking
/// pool.
pub async fn agent_tmux_session_is_alive(os_user: &str, session_id: &str) -> Result<bool, Status> {
    let (os_user, name) = (os_user.to_string(), agent_tmux_session_name(session_id));
    in_tmux_blocking(move || tmux_session_alive(&os_user, &name)).await
}

/// Re-attach `session_id`'s main terminal from what its session directory records, when the session
/// ran in tmux and its tmux session is alive — the lazy re-attach a first stream triggers after a
/// daemon restart or a client's exit. Dispatched by the recorded session type: claude-cli here
/// (its main recording registered as a start registers it), every other CLI harness through
/// [`crate::cli_harness_tmux::reattach_recorded_harness`]. `false` when the record is unreadable,
/// the session did not run in tmux, its tmux session is gone, its agent has exited or the attach
/// failed (logged); nothing is relaunched.
pub async fn reattach_recorded_main(
    manager: &CliSessionManager,
    tddy_data_dir: &Path,
    os_user: &str,
    session_id: &str,
) -> bool {
    use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
    if validate_session_id_segment(session_id).is_err() {
        return false;
    }
    let session_dir = unified_session_dir_path(tddy_data_dir, session_id);
    let Ok(meta) = tddy_core::read_session_metadata(&session_dir) else {
        return false;
    };
    if meta.tmux_session.is_none() {
        return false;
    }
    match agent_tmux_session_is_alive(os_user, session_id).await {
        Ok(true) => {}
        Ok(false) => return false,
        Err(error) => {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not tell whether the tmux session of {session_id} is alive: {}",
                error.message()
            );
            return false;
        }
    }
    let worktree_path =
        tddy_core::repo_root_for_session(&session_dir).unwrap_or_else(|| session_dir.clone());
    let reattached = match meta.session_type.as_deref().unwrap_or("claude-cli") {
        "claude-cli" => {
            let launch = ClaudeCliTmuxLaunch {
                os_user,
                session_id,
                worktree_path,
                resume: true,
                ..Default::default()
            };
            match reattach_claude_in_tmux(manager, launch).await {
                Ok(_) => {
                    crate::claude_cli_spawn::record_main_terminal(
                        manager,
                        tddy_data_dir,
                        session_id,
                    )
                    .await;
                    Ok(true)
                }
                Err(error) => Err(error),
            }
        }
        // Every other CLI harness re-attaches through its own tmux path, which watches its
        // agent's exit again (`#tmux-agent` 3/3).
        session_type => {
            crate::cli_harness_tmux::reattach_recorded_harness(
                manager,
                crate::cli_harness_tmux::RecordedHarness {
                    os_user,
                    session_id,
                    session_type,
                    worktree_path,
                    session_dir: &session_dir,
                },
            )
            .await
        }
    };
    match reattached {
        Ok(attached) => attached,
        Err(error) => {
            log::warn!(
                target: "tddy_daemon::claude_cli_session",
                "could not re-attach the main terminal of {session_id}: {}",
                error.message()
            );
            false
        }
    }
}

/// Relaunch a resumed claude-cli agent and return the pid to record. A session that ran in tmux
/// (`in_tmux`) re-attaches to its tmux session when that still lives, and otherwise starts a fresh
/// one; a session that did not run in tmux is relaunched as the PTY child, exactly as before.
pub async fn relaunch_claude_cli(
    manager: &CliSessionManager,
    in_tmux: bool,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<u32, Status> {
    if in_tmux {
        // A tmux session still alive holds the agent the session recorded: the client re-attaches to
        // it. Only a gone one is started again.
        let alive = agent_tmux_session_is_alive(launch.os_user, launch.session_id).await?;
        let launched = if alive {
            reattach_claude_in_tmux(manager, launch).await
        } else {
            resume_claude_in_tmux(manager, launch).await
        };
        return launched.map(|l| l.pane_pid);
    }
    manager
        .resume_with_options(
            launch.session_id,
            launch.worktree_path,
            launch.model,
            launch.binary,
            launch.append_system_prompt_file,
            launch.extra_args,
            launch.env,
        )
        .await
        .map(|handle| handle.pid)
        .map_err(|e| Status::internal(format!("failed to relaunch claude-cli: {e}")))
}
