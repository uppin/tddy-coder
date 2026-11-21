//! Acceptance tests: a target session's agent tab opened "On target" runs the agent on the session's
//! target — the target its owner's credentials reach, found by its bare command on the target's
//! login `PATH`, in the session's worktree there, in a conversation of its own — is listed under its
//! harness, and is recorded `[target, agent]` (`#session-liveness` 8/8).
//!
//! Extended by `#tab-hardening` 1/4: a failed start says what went wrong with a code a client can
//! act on — an unreachable target `UNAVAILABLE`, an agent the target lacks `FAILED_PRECONDITION`
//! naming it, a local terminal that cannot spawn `INTERNAL` — and records nothing
//! (PRD-2026-11-21-tab-errors.md).
//!
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs)
//!
//! The session's target resolves through an in-memory `TabTargetShell`
//! (`target_tab_support::fake_target_shells`), injected with
//! `DaemonSessionHost::with_tab_target_shell`, to a machine on this host standing for the target,
//! with Claude Code installed there. Everything else is the daemon's own: the terminal RPCs, the
//! roster, the agent-tab rule and the target tab's launch.

mod support;
mod target_tab_support;

use pretty_assertions::assert_eq;
use tddy_agent_launch::remote_agent_launch::RemoteLaunchError;
use tddy_core::tab_roster::LaunchNode;
use tddy_rpc::Code;

use target_tab_support::fake_target_shells::{
    target_shells_reaching_a_machine, target_shells_reaching_an_unreachable_machine,
    target_shells_whose_client_this_host_lacks, A_CLIENT_THIS_HOST_LACKS, UNREACHABLE,
};
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

#[tokio::test]
async fn an_agent_tab_on_an_unreachable_target_is_refused_unavailable() {
    // Given a target session whose target resolves to a machine its transport cannot reach
    let (target_shells, machine) = target_shells_reaching_an_unreachable_machine();
    let daemon = a_target_session_whose_shells_resolve_through(target_shells);

    // When a Claude tab is opened "On target"
    let refusal = daemon
        .opens_a_tab_on_target("claude-cli")
        .await
        .expect_err("an agent tab on an unreachable target fails");

    // Then the target, not the request, is at fault: UNAVAILABLE carrying what the transport said,
    // nothing was opened there, and nothing is recorded
    assert_eq!(
        (refusal.code(), refusal.message().contains(UNREACHABLE)),
        (Code::Unavailable, true),
        "the refusal: {refusal:?}"
    );
    assert_eq!(machine.interactive_commands(), Vec::<String>::new());
    assert_eq!(daemon.roster().tabs, Vec::new());
}

#[tokio::test]
async fn an_agent_tab_whose_agent_is_missing_on_the_target_is_refused_naming_it() {
    // Given a target session whose target is a reachable machine with no Claude Code installed
    let (target_shells, machine) = target_shells_reaching_a_machine();
    let daemon = a_target_session_whose_shells_resolve_through(target_shells);

    // When a Claude tab is opened "On target"
    let refusal = daemon
        .opens_a_tab_on_target("claude-cli")
        .await
        .expect_err("an agent tab whose agent the target lacks fails");

    // Then it is refused as a precondition the caller can meet, naming the agent the target lacks,
    // before any terminal was opened there, and nothing is recorded
    assert_eq!(
        (refusal.code(), refusal.message().to_string()),
        (
            Code::FailedPrecondition,
            RemoteLaunchError::AgentNotOnTarget {
                command: "claude".to_string()
            }
            .to_string()
        )
    );
    assert_eq!(machine.interactive_commands(), Vec::<String>::new());
    assert_eq!(daemon.roster().tabs, Vec::new());
}

#[tokio::test]
async fn a_tab_whose_terminal_cannot_spawn_is_refused_internal() {
    // Given a target session whose target is a reachable machine with Claude Code installed, but
    // whose transport client this host does not have
    let (target_shells, machine) = target_shells_whose_client_this_host_lacks();
    machine.installs_the_agent("claude");
    let daemon = a_target_session_whose_shells_resolve_through(target_shells);

    // When a Claude tab is opened "On target"
    let refusal = daemon
        .opens_a_tab_on_target("claude-cli")
        .await
        .expect_err("a tab whose terminal cannot spawn fails");

    // Then this host, not the caller, is at fault: INTERNAL naming the client that could not be
    // spawned, the target's agent never ran, and nothing is recorded
    assert_eq!(
        (
            refusal.code(),
            refusal.message().contains(A_CLIENT_THIS_HOST_LACKS)
        ),
        (Code::Internal, true),
        "the refusal: {refusal:?}"
    );
    assert_eq!(machine.agent_run("claude"), None);
    assert_eq!(daemon.roster().tabs, Vec::new());
}
