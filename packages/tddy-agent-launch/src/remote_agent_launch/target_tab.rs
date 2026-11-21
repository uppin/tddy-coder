//! A **tab** on a session's target (`#session-liveness` 6/8): the target user's login shell, in the
//! session's worktree there, run through the session's remote shell by a local PTY that is one of
//! the session's tabs — never its main.
//!
//! It shares the transport client with [`super::launch_on_target`] (the local PTY runs
//! [`RemoteShell::interactive`] over the tab's command, with the client's own environment) and
//! nothing of the main's start: no pid file is written on the target, no session pointer is placed
//! or rewritten, there is no startup window, and nothing it does stops the session. A target it
//! cannot reach fails the tab alone, before any terminal is registered.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md

use std::path::Path;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_remote_shell::RemoteShell;

use super::RemoteLaunchError;

/// Where a tab is opened on a target.
pub struct TargetTab<'a> {
    /// The shell that reaches the session's target.
    pub shell: Arc<dyn RemoteShell>,
    /// The session's worktree on the target: the tab's working directory there.
    pub worktree: &'a Path,
    /// The session's directory on this host: the working directory of the local PTY that runs the
    /// transport's client.
    pub session_dir: &'a Path,
}

/// The one shell command a shell tab runs on a target: enter `worktree`, then `exec` the target
/// user's own login shell there — nothing is written on the target first:
///
/// ```text
/// cd '<worktree>' && exec "${SHELL:-/bin/sh}" -l
/// ```
///
/// The worktree is single-quoted for a POSIX shell. The login shell is the target's `$SHELL`, run as
/// a login shell with the target login's own environment: a person's shell, not an agent's
/// `env -i` launch ([`super::target_login_exec`]).
pub fn target_shell_tab_command(worktree: &Path) -> String {
    // TODO(target-shell-tab): implement
    let _ = worktree;
    todo!("the remote command of a shell tab on a target")
}

/// Open the shell tab `terminal_id` of `session_id` on the target `tab.shell` reaches, in
/// `tab.worktree` there, and register the local PTY that is its terminal with `manager`.
///
/// The target is reached first, through the port, with `tab.worktree` entered there: an unreachable
/// target, or a worktree it does not hold, fails the tab with the shell's own reason
/// ([`RemoteLaunchError::Shell`]) and registers nothing. Then the local PTY runs
/// [`RemoteShell::interactive`] over [`target_shell_tab_command`] in `tab.session_dir`, as a tab
/// (`CliSessionManager::spawn_rendered_client_tab`), and the launch returns with it — there is no
/// startup window to wait out. Nothing of the session's main is touched, whatever happens.
pub async fn launch_tab_on_target(
    manager: &CliSessionManager,
    session_id: &str,
    terminal_id: &str,
    tab: TargetTab<'_>,
) -> Result<Arc<PtyHandle>, RemoteLaunchError> {
    // TODO(target-shell-tab): implement
    let _ = (manager, session_id, terminal_id, tab);
    todo!("launch a shell tab on the session's target")
}

#[cfg(test)]
#[path = "target_tab_tests.rs"]
mod tests;
