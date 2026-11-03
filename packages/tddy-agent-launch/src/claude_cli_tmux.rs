//! Running a Claude CLI agent inside tmux (`#tmux-agent` 1/3,
//! docs/ft/daemon/1-WIP/PRD-2026-11-03-claude-cli-in-tmux.md).
//!
//! The agent runs in a tmux session of its own, as the session's OS user; the session's main
//! terminal is a tmux client attached to it, and the pid the session records is the pane's process —
//! so liveness, signal and delete act on the agent, not on the client.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::tmux::{
    agent_tmux_session_name, kill_tmux_session, start_agent_tmux_session,
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
    /// Set on the tmux *pane* (`new-session -e`): a pane inherits the tmux server's environment,
    /// not the client's.
    pub env: Vec<(String, String)>,
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
/// The pane is started with `launch.env` on it, and its pid is the agent's.
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
    let pane_pid = {
        let (os_user, name, worktree) = (os_user.to_string(), name.clone(), worktree_path.clone());
        in_tmux_blocking(move || start_agent_tmux_session(&os_user, &name, &worktree, &env, &argv))
            .await?
            .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {e}")))?
    };
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
async fn in_tmux_blocking<T, F>(operation: F) -> Result<T, Status>
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
async fn stop_tmux_session_of_failed_start(os_user: &str, name: &str) {
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

/// Relaunch a resumed claude-cli agent and return the pid to record: in a fresh tmux session when
/// the session ran in tmux (`in_tmux`), else as the PTY child, exactly as before.
pub async fn relaunch_claude_cli(
    manager: &CliSessionManager,
    in_tmux: bool,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<u32, Status> {
    if in_tmux {
        return resume_claude_in_tmux(manager, launch)
            .await
            .map(|l| l.pane_pid);
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
