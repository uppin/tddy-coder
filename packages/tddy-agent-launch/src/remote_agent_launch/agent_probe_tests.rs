//! Unit tests: the probe of an agent on a target ([`probe_agent_on_target`]) — one call through the
//! port asking the target's login shell for the agent's bare command in the session's worktree, its
//! paths single-quoted, and what its answer means.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_remote_shell::{ExecOutput, InMemoryShell, RemoteShellError, ShellAddress};

use super::probe_agent_on_target;
use crate::remote_agent_launch::RemoteLaunchError;

const WORKTREE: &str = "/home/dev/repo/.worktrees/s1";

/// What the probe of `claude` in [`WORKTREE`] runs on the target.
const PROBE_OF_CLAUDE: &str = r#"cd '/home/dev/repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'command -v '"'"'claude'"'"''"#;

/// A reachable target named `buildbox`.
fn a_target() -> InMemoryShell {
    InMemoryShell::new("buildbox")
}

#[tokio::test]
async fn the_probe_asks_the_targets_login_shell_for_the_command_in_the_worktree() {
    // Given a target whose login shell finds Claude Code
    let target = a_target();
    target.answers(
        PROBE_OF_CLAUDE,
        ExecOutput::succeeded("/home/dev/.local/bin/claude\n"),
    );

    // When
    let probed = probe_agent_on_target(&target, Path::new(WORKTREE), "claude").await;

    // Then it ran exactly the one probe, and found the agent
    assert_eq!(
        (probed, target.commands_run()),
        (Ok(()), vec![PROBE_OF_CLAUDE.to_string()])
    );
}

#[tokio::test]
async fn the_probe_single_quotes_a_worktree_holding_a_space_and_a_quote() {
    // Given a worktree whose path holds a space and a single quote
    let worktree = "/home/dev/ada's repo/.worktrees/s1";
    let probe = r#"cd '/home/dev/ada'"'"'s repo/.worktrees/s1' && exec "${SHELL:-/bin/sh}" -lc 'command -v '"'"'claude'"'"''"#;
    let target = a_target();
    target.answers(probe, ExecOutput::succeeded("/usr/local/bin/claude\n"));

    // When
    let probed = probe_agent_on_target(&target, Path::new(worktree), "claude").await;

    // Then
    assert_eq!(
        (probed, target.commands_run()),
        (Ok(()), vec![probe.to_string()])
    );
}

#[tokio::test]
async fn an_agent_the_targets_login_shell_does_not_find_is_not_on_the_target() {
    // Given a target whose login shell finds no Claude Code
    let target = a_target();
    target.answers(PROBE_OF_CLAUDE, ExecOutput::exited(1, ""));

    // When
    let probed = probe_agent_on_target(&target, Path::new(WORKTREE), "claude").await;

    // Then
    assert_eq!(
        probed,
        Err(RemoteLaunchError::AgentNotOnTarget {
            command: "claude".to_string()
        })
    );
}

#[tokio::test]
async fn a_probe_of_a_target_its_transport_cannot_reach_fails_with_the_transports_error() {
    // Given
    let target = a_target();
    target.is_unreachable("Could not resolve hostname buildbox");

    // When
    let probed = probe_agent_on_target(&target, Path::new(WORKTREE), "claude").await;

    // Then the target, not the agent, is at fault
    assert_eq!(
        probed,
        Err(RemoteLaunchError::Shell(RemoteShellError::Unreachable {
            address: ShellAddress::new(InMemoryShell::TRANSPORT, "buildbox"),
            detail: "Could not resolve hostname buildbox".to_string(),
        }))
    );
}
