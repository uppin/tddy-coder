//! The check made before an agent is started on a target (`#tab-hardening` 1/4): that the target's
//! own login shell finds the agent's bare command on its `PATH`. An agent tab — new or restored —
//! makes it before its terminal starts (and the target start of `#tab-hardening` 4/4 reuses it), so
//! a target without the agent fails the launch naming the command, instead of opening a terminal
//! whose shell exits 127.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-tab-errors.md

use std::path::Path;

use tddy_remote_shell::RemoteShell;

use super::RemoteLaunchError;

/// Ask the target `shell` reaches whether its login shell, in `worktree`, finds `command` — an
/// agent's bare command, as its spec names it (`claude`, `agent`) — with one call through the port:
///
/// ```text
/// cd '<worktree>' && exec "${SHELL:-/bin/sh}" -lc 'command -v '\''<command>'\'''
/// ```
///
/// The worktree and the command are single-quoted for a POSIX shell, and the command is looked up
/// by the target's own login shell, on the target login's `PATH` — where the agent's launch
/// ([`super::target_login_exec`]) will look for it. A probe that exits non-zero is
/// [`RemoteLaunchError::AgentNotOnTarget`] naming `command`; a target the transport cannot reach,
/// or a probe that gives no answer within its deadline, is [`RemoteLaunchError::Shell`].
pub async fn probe_agent_on_target(
    shell: &dyn RemoteShell,
    worktree: &Path,
    command: &str,
) -> Result<(), RemoteLaunchError> {
    // TODO(tab-errors): implement
    let _ = (shell, worktree, command);
    todo!()
}

#[cfg(test)]
#[path = "agent_probe_tests.rs"]
mod tests;
