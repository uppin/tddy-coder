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
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs)

use std::path::Path;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ClientTab};
use tddy_core::ssh_exec::shell_single_quote;
use tddy_remote_shell::{ExecRequest, RemoteShell};

use super::{transport_client, RemoteLaunchError};

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
    format!(
        "cd {} && exec \"${{SHELL:-/bin/sh}}\" -l",
        shell_single_quote(&worktree.to_string_lossy())
    )
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
    reach_the_worktree(&tab).await?;
    spawn_tab_client(
        manager,
        session_id,
        terminal_id,
        &tab,
        "bash",
        &target_shell_tab_command(tab.worktree),
    )
    .await
}

/// Reach the target first, in the worktree: an unreachable target (or a worktree it lacks) fails the
/// tab, with the transport's words, before any terminal is registered.
pub(super) async fn reach_the_worktree(tab: &TargetTab<'_>) -> Result<(), RemoteLaunchError> {
    tab.shell
        .exec(ExecRequest::new(format!(
            "cd {}",
            shell_single_quote(&tab.worktree.to_string_lossy())
        )))
        .await
        .map_err(RemoteLaunchError::Shell)?
        .success_or_exited()
        .map_err(RemoteLaunchError::Shell)?;
    Ok(())
}

/// Register the local PTY that runs the transport's client over `command` as the tab `terminal_id`
/// of kind `kind` (`CliSessionManager::spawn_rendered_client_tab`), in `tab.session_dir`.
pub(super) async fn spawn_tab_client(
    manager: &CliSessionManager,
    session_id: &str,
    terminal_id: &str,
    tab: &TargetTab<'_>,
    kind: &str,
    command: &str,
) -> Result<Arc<PtyHandle>, RemoteLaunchError> {
    let client = transport_client(tab.shell.as_ref(), command);
    manager
        .spawn_rendered_client_tab(
            session_id,
            terminal_id,
            tab.session_dir.to_path_buf(),
            ClientTab {
                kind: kind.to_string(),
                argv: client.argv,
                env: client.env,
                exit_code_in_output: client.exit_code_in_output,
            },
        )
        .await
        .map_err(|error| RemoteLaunchError::Spawn(error.to_string()))
}

#[cfg(test)]
#[path = "target_tab_tests.rs"]
mod tests;
