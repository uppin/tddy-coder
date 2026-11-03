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
use tddy_rpc::Status;

pub use crate::claude_cli_tmux::TmuxLaunched;

/// What a tmux-run harness is launched with.
pub struct HarnessTmuxLaunch<'a> {
    pub os_user: &'a str,
    pub session_id: &'a str,
    pub worktree_path: PathBuf,
    /// The main terminal's kind: the session type (`cursor-cli`, `warp-cli`, ...).
    pub kind: &'a str,
    /// The harness's complete argv, binary first.
    pub argv: Vec<String>,
    /// Set on the tmux *pane* (`new-session -e`): a pane inherits the tmux server's environment.
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
    // TODO(harnesses-in-tmux): implement — `start_agent_tmux_session(.., remain_on_exit =
    // on_exit.is_some())`, `attach_main_to_tmux`, then the exit-hook source.
    let _ = (manager, launch);
    unimplemented!("TODO(harnesses-in-tmux): start_harness_in_tmux")
}

/// Resume a tmux-run harness session: re-attach to its tmux session when that is still alive
/// (`CliSessionManager::reattach_main`), else kill the leftover and start fresh with the harness's
/// resume argv. `launch.argv` is the resume argv.
pub async fn resume_harness_in_tmux(
    manager: &CliSessionManager,
    launch: HarnessTmuxLaunch<'_>,
) -> Result<TmuxLaunched, Status> {
    // TODO(harnesses-in-tmux): implement — `tmux_session_alive` → `reattach_main` + `pane_pid`,
    // else `kill_tmux_session` and as a start.
    let _ = (manager, launch);
    unimplemented!("TODO(harnesses-in-tmux): resume_harness_in_tmux")
}
