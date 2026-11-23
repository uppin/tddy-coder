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
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs)

use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::ssh_exec::shell_single_quote;

use super::agent_probe::probe_agent_on_target;
use super::session_files::{write_rendered_session_files, SessionFilesPlace};
use super::target_tab::{reach_the_worktree, spawn_tab_client};
use super::{target_login_exec, RemoteLaunchError, TargetTab};
use crate::spec_launch::HostPtyPlan;

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
    /// The tmux session the agent runs in on the target (`#target-tmux` 2/2): the tab attaches to it
    /// when it lives, and starts the agent in it when it does not. `None` runs the agent straight
    /// under the tab's transport client.
    pub tmux_session: Option<String>,
}

/// Open the agent tab `terminal_id` of `session_id` on the target `tab.shell` reaches, running
/// `agent` in `tab.worktree` there, and register the local PTY that is its terminal with `manager`
/// under `agent.kind`.
///
/// The target is reached first, through the port, with `tab.worktree` entered there: an unreachable
/// target, or a worktree it does not hold, fails the tab with the shell's own reason
/// ([`RemoteLaunchError::Shell`]) and registers nothing. Then the target's login shell is asked for
/// the agent's bare command ([`probe_agent_on_target`]): an agent it does not find fails the tab
/// [`RemoteLaunchError::AgentNotOnTarget`], before any terminal is started. A file `agent.argv`
/// names in this host's session directory (`tab.session_dir`) is written into the worktree's
/// `.tddy/` on the target and its argument rewritten to that copy, as a main's launch writes its
/// rendered session files — so no path of this host reaches the target. Then the local PTY runs
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
    reach_the_worktree(&tab).await?;
    // An agent the target's login `PATH` lacks fails the tab naming it, before its terminal starts.
    let command = agent.argv.first().map_or("", String::as_str);
    probe_agent_on_target(tab.shell.as_ref(), tab.worktree, command).await?;

    // A file the tab's argv names in this session's directory goes to the worktree's `.tddy/` on the
    // target, its argument rewritten: no host path reaches the target.
    let place = SessionFilesPlace {
        shell: tab.shell.as_ref(),
        worktree: tab.worktree,
        session_dir: tab.session_dir,
    };
    let plan = write_rendered_session_files(
        &place,
        Vec::new(),
        HostPtyPlan {
            argv: agent.argv.clone(),
            env: agent.env.clone(),
        },
    )
    .await?;

    let command = format!(
        "cd {} && {}",
        shell_single_quote(&tab.worktree.to_string_lossy()),
        target_login_exec(&plan.argv, &plan.env)
    );
    spawn_tab_client(
        manager,
        session_id,
        terminal_id,
        &tab,
        &agent.kind,
        &command,
    )
    .await
}
