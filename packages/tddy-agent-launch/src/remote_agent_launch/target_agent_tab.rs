//! An **agent tab** on a session's target (`#session-liveness` 8/8): the agent run on the target, in
//! the session's worktree there, by a local PTY that is one of the session's tabs — never its main.
//!
//! It is the agent sibling of [`super::launch_tab_on_target`] (the shell tab): the same transport
//! client over [`RemoteShell::interactive`](tddy_remote_shell::RemoteShell::interactive), the same
//! non-main local PTY, and nothing of the main's start — no pid file on the target, no session
//! pointer, no startup window, and nothing it does stops the session. What it runs is the main's
//! kind of command instead of a login shell: `cd '<worktree>' && <target_login_exec>` — the agent
//! found by its bare command on the target login's own `PATH`, under `env -i` with exactly the tab's
//! environment ([`super::target_login_exec`]).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-agent-tab.md

use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;

use super::{RemoteLaunchError, TargetTab};

/// What an agent tab runs on a target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetAgentTab {
    /// The harness's session type (`claude-cli`): the tab's kind, as a listing reports it.
    pub kind: String,
    /// The agent's argv, its spec's **bare command** first (`claude`, `agent`): what the target's
    /// login `PATH` resolves. Never a path this host resolved.
    pub argv: Vec<String>,
    /// The agent's environment, carried inside the remote command because a remote shell forwards
    /// none: the spec's `run.env`, and the `pass_env` values the daemon holds. The base set (`HOME`,
    /// `PATH`, `TMPDIR`) is the target's own.
    pub env: Vec<(String, String)>,
}

/// Open the agent tab `terminal_id` of `session_id` on the target `tab.shell` reaches, running
/// `agent` in `tab.worktree` there, and register the local PTY that is its terminal with `manager`
/// under `agent.kind`.
///
/// The target is reached first, through the port, with `tab.worktree` entered there: an unreachable
/// target, or a worktree it does not hold, fails the tab with the shell's own reason
/// ([`RemoteLaunchError::Shell`]) and registers nothing. A file `agent.argv` names in this host's
/// session directory (`tab.session_dir`) is written into the worktree's `.tddy/` on the target and
/// its argument rewritten to that copy, as a main's launch writes its rendered session files — so
/// no path of this host reaches the target. Then the local PTY runs
/// [`RemoteShell::interactive`](tddy_remote_shell::RemoteShell::interactive) over
/// `cd '<worktree>' && <target_login_exec(argv, env)>` in `tab.session_dir`, as a tab
/// (`CliSessionManager::spawn_rendered_client_tab`), and the launch returns with it. Nothing of the
/// session's main is touched, whatever happens.
pub async fn launch_agent_tab_on_target(
    manager: &CliSessionManager,
    session_id: &str,
    terminal_id: &str,
    tab: TargetTab<'_>,
    agent: &TargetAgentTab,
) -> Result<Arc<PtyHandle>, RemoteLaunchError> {
    // TODO(target-agent-tab): implement
    let _ = (manager, session_id, terminal_id, tab, agent);
    todo!("launch an agent tab on the session's target")
}
