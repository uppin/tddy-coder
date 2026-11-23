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
use tddy_remote_shell::RemoteShell;
use tddy_terminal_rpc::TmuxSession;

use super::{RemoteLaunchError, TargetTab};

/// The tmux sessions of the target login `shell` reaches, listed through its login shell from
/// `worktree`: empty when no tmux server runs there, [`RemoteLaunchError::TmuxNotOnTarget`] when its
/// login shell finds no `tmux`.
pub async fn list_target_tmux_sessions(
    shell: &dyn RemoteShell,
    worktree: &Path,
) -> Result<Vec<TmuxSession>, RemoteLaunchError> {
    let _ = (shell, worktree);
    todo!("#target-tmux 2/2: list the target's tmux sessions")
}

/// The command a tmux tab runs on a target: `tmux new-session -A -s <name>` in `worktree`, through
/// the target's login shell, running `inner` (an already-quoted shell command) as the new session's
/// command when it is made, or the login's shell when `inner` is `None`.
pub fn target_tmux_tab_command(worktree: &Path, name: &str, inner: Option<&str>) -> String {
    let _ = (worktree, name, inner);
    todo!("#target-tmux 2/2: the target tmux tab command")
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
    let _ = (manager, session_id, terminal_id, tab, name);
    todo!("#target-tmux 2/2: a tmux tab on the target")
}

/// Kill the tmux session `name` on the target `shell` reaches, through its login shell. A tmux
/// session — or server — already gone is not an error.
pub async fn end_target_tmux_session_named(
    shell: &dyn RemoteShell,
    name: &str,
) -> Result<(), RemoteLaunchError> {
    let _ = (shell, name);
    todo!("#target-tmux 2/2: kill a tmux session on the target by name")
}

/// Whether the target `shell` reaches holds the tmux session `name`.
pub async fn target_tmux_session_lives(
    shell: &dyn RemoteShell,
    name: &str,
) -> Result<bool, RemoteLaunchError> {
    let _ = (shell, name);
    todo!("#target-tmux 2/2: whether a tmux session lives on the target")
}

#[cfg(test)]
#[path = "target_tmux_tab_tests.rs"]
mod tests;
