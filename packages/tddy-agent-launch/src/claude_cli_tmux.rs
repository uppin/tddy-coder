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
use tddy_rpc::Status;

/// What a tmux-run claude is launched with: the arguments [`CliSessionManager::start_with_options`]
/// takes for a PTY-child launch, so the two launches build the same `claude` argv.
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
pub async fn start_claude_in_tmux(
    manager: &CliSessionManager,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    // TODO(claude-in-tmux): implement — build the claude argv (as `start_with_options` does),
    // `tddy_cli_sessions::tmux::start_agent_tmux_session`, then `manager.attach_main_to_tmux`.
    let _ = (manager, launch);
    unimplemented!("TODO(claude-in-tmux): start_claude_in_tmux")
}

/// Resume a tmux-run session: kill the leftover tmux session of the terminated run, then start
/// claude in a fresh one with the resume argv. `launch.resume` is true.
pub async fn resume_claude_in_tmux(
    manager: &CliSessionManager,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    // TODO(claude-in-tmux): implement — `kill_tmux_session` for the leftover, then as a start.
    let _ = (manager, launch);
    unimplemented!("TODO(claude-in-tmux): resume_claude_in_tmux")
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
