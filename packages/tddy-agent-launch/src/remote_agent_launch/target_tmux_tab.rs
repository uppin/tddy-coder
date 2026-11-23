//! A target session's **tmux tabs on the target** (`#target-tmux` 2/2): the target login's tmux
//! sessions listed through its login shell, and a tab that is a client of a tmux session there — a
//! tmux session a person keeps, one the tab names anew, or one the tab's agent runs in.
//!
//! Every tab runs one command through the target's login shell, in the session's worktree there:
//!
//! ```text
//! cd '<worktree>' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux new-session -A -s '\''<name>'\'' [<inner>]'
//! ```
//!
//! `new-session -A` attaches to `<name>` when it lives and makes it otherwise, so a start, a re-attach
//! and a recreate are the same command.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md (n2)

use std::path::Path;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::tmux::{parse_list_sessions, LIST_SESSIONS_FORMAT};
use tddy_core::ssh_exec::shell_single_quote;
use tddy_remote_shell::{ExecRequest, RemoteShell, RemoteShellError};
use tddy_terminal_rpc::TmuxSession;

use super::agent_probe::probe_tmux_on_target;
use super::target_tab::{reach_the_worktree, spawn_tab_client};
use super::{target_login_run, RemoteLaunchError, TargetTab};

/// The kind a tmux tab on a target is listed under.
const TMUX_TAB_KIND: &str = "tmux";

/// What tmux says when no server runs for the login: an empty listing, not a failure.
fn no_tmux_server(stderr: &str) -> bool {
    let lowered = stderr.to_lowercase();
    lowered.contains("no server running") || lowered.contains("error connecting")
}

/// `inner` run by the target's login shell in `worktree`.
fn in_the_worktree(worktree: &Path, inner: &str) -> String {
    format!(
        "cd {} && {}",
        shell_single_quote(&worktree.to_string_lossy()),
        target_login_run(inner)
    )
}

/// The tmux sessions of the target login `shell` reaches, listed through its login shell from
/// `worktree`: empty when no tmux server runs there, [`RemoteLaunchError::TmuxNotOnTarget`] when its
/// login shell finds no `tmux`.
pub async fn list_target_tmux_sessions(
    shell: &dyn RemoteShell,
    worktree: &Path,
) -> Result<Vec<TmuxSession>, RemoteLaunchError> {
    probe_tmux_on_target(shell, worktree).await?;
    let listing = in_the_worktree(
        worktree,
        &format!(
            "exec tmux list-sessions -F {}",
            shell_single_quote(LIST_SESSIONS_FORMAT)
        ),
    );
    let output = shell
        .exec(ExecRequest::new(listing))
        .await
        .map_err(RemoteLaunchError::Shell)?;
    if output.status == 0 {
        return Ok(parse_list_sessions(&String::from_utf8_lossy(
            &output.stdout,
        )));
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if no_tmux_server(&stderr) {
        return Ok(Vec::new());
    }
    Err(RemoteLaunchError::Shell(RemoteShellError::Exited {
        status: output.status,
        stderr,
    }))
}

/// The command a tmux tab runs on a target: `tmux new-session -A -s <name>` in `worktree`, through
/// the target's login shell, running `inner` (an already-quoted shell command) as the new session's
/// command when it is made, or the login's shell when `inner` is `None`.
pub fn target_tmux_tab_command(worktree: &Path, name: &str, inner: Option<&str>) -> String {
    let mut tmux = format!("exec tmux new-session -A -s {}", shell_single_quote(name));
    if let Some(inner) = inner {
        tmux.push(' ');
        tmux.push_str(&shell_single_quote(inner));
    }
    in_the_worktree(worktree, &tmux)
}

/// Open the tab `terminal_id` of `session_id` as a client of the tmux session `name` on the target
/// `tab.shell` reaches — attached when it lives, made in `tab.worktree` otherwise — and register the
/// local PTY that is its terminal with `manager`. The target is reached first and asked for tmux:
/// an unreachable target fails [`RemoteLaunchError::Shell`], a target without tmux
/// [`RemoteLaunchError::TmuxNotOnTarget`], before any terminal is registered.
pub async fn launch_tmux_tab_on_target(
    manager: &CliSessionManager,
    session_id: &str,
    terminal_id: &str,
    tab: TargetTab<'_>,
    name: &str,
) -> Result<Arc<PtyHandle>, RemoteLaunchError> {
    reach_the_worktree(&tab).await?;
    probe_tmux_on_target(tab.shell.as_ref(), tab.worktree).await?;
    let command = target_tmux_tab_command(tab.worktree, name, None);
    spawn_tab_client(
        manager,
        session_id,
        terminal_id,
        &tab,
        TMUX_TAB_KIND,
        &command,
    )
    .await
}

/// Kill the tmux session `name` on the target `shell` reaches, through its login shell. A tmux
/// session — or server — already gone is not an error.
pub async fn end_target_tmux_session_named(
    shell: &dyn RemoteShell,
    name: &str,
) -> Result<(), RemoteLaunchError> {
    let name = shell_single_quote(&format!("={name}"));
    // A session or server already gone is what the end wants; any other refusal is reported.
    let inner = format!(
        "out=$(tmux kill-session -t {name} 2>&1) && exit 0; \
         case \"$out\" in *\"can't find session\"*|*\"no server running\"*|*\"error connecting to\"*) exit 0;; esac; \
         printf '%s\\n' \"$out\" >&2; exit 1"
    );
    shell
        .exec(ExecRequest::new(target_login_run(&inner)))
        .await
        .map_err(RemoteLaunchError::Shell)?
        .success_or_exited()
        .map_err(RemoteLaunchError::Shell)?;
    Ok(())
}

/// Whether the target `shell` reaches holds the tmux session `name`.
pub async fn target_tmux_session_lives(
    shell: &dyn RemoteShell,
    name: &str,
) -> Result<bool, RemoteLaunchError> {
    let inner = format!(
        "exec tmux has-session -t {}",
        shell_single_quote(&format!("={name}"))
    );
    let output = shell
        .exec(ExecRequest::new(target_login_run(&inner)))
        .await
        .map_err(RemoteLaunchError::Shell)?;
    Ok(output.status == 0)
}

#[cfg(test)]
#[path = "target_tmux_tab_tests.rs"]
mod tests;
