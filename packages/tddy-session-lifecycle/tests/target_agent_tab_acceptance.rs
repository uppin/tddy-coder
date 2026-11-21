//! Acceptance tests: a target session's agent tab opened "On target" runs the agent on the session's
//! target — the target its owner's credentials reach, found by its bare command on the target's
//! login `PATH`, in the session's worktree there, in a conversation of its own — is listed under its
//! harness, and is recorded `[target, agent]` (`#session-liveness` 8/8).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-target-agent-tab.md
//!
//! The session's target resolves through an in-memory `TabTargetShell`
//! (`target_tab_support::fake_target_shells`), injected with
//! `DaemonSessionHost::with_tab_target_shell`, to a machine on this host standing for the target,
//! with Claude Code installed there. Everything else is the daemon's own: the terminal RPCs, the
//! roster, the agent-tab rule and the target tab's launch.

mod support;
mod target_tab_support;

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::LaunchNode;

use target_tab_support::fake_target_shells::target_shells_reaching_a_machine;
use target_tab_support::{
    a_target_session_whose_shells_resolve_through, where_the_agent_ran, wire_kinds_of,
};

#[tokio::test]
async fn a_target_agent_tab_is_recorded_as_target_then_agent() {
    // Given a target session whose target is a reachable machine with Claude Code installed
    let (target_shells, machine) = target_shells_reaching_a_machine();
    machine.installs_the_agent("claude");
    let daemon = a_target_session_whose_shells_resolve_through(target_shells);

    // When a Claude tab is opened "On target"
    let tab = daemon
        .opens_a_tab_on_target("claude-cli")
        .await
        .expect("an agent tab starts on the target");

    // Then the session's target was resolved as its owner, and the target's Claude runs in the
    // session's worktree there, starting a conversation of its own ...
    assert_eq!(
        daemon.target_shells.asked(),
        daemon.the_sessions_owner_asked(1)
    );
    let (cwd, args) = where_the_agent_ran(&machine, "claude").await;
    assert_eq!(
        (cwd, args.first().map(String::as_str)),
        (machine.worktree(), Some("--session-id"))
    );
    let conversation = args.get(1).cloned().unwrap_or_default();
    // ... and the tab is recorded on the target holding that conversation, and listed as a Claude
    // tab run on the target
    assert_eq!(
        daemon.roster_entry(&tab).launch,
        vec![
            LaunchNode::target(),
            LaunchNode::agent("claude-cli", &conversation)
        ]
    );
    let listed = daemon.listed_terminal(&tab).await;
    assert_eq!(
        (listed.kind.as_str(), wire_kinds_of(&listed)),
        ("claude-cli", vec!["target", "agent"])
    );
}
