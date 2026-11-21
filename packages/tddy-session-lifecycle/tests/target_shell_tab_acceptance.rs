//! Acceptance tests: a target session's shell tab opened "On target" runs on the session's target —
//! the target its owner's credentials reach, in the session's worktree there — is listed, and is
//! recorded `[target, shell]`; a tab opened "On this host" is a plain shell here, in the session's
//! own directory; a target that resolves but cannot be reached fails the tab `UNAVAILABLE` with the
//! transport's words, and one that cannot be resolved (a locked vault) `FAILED_PRECONDITION`. An
//! agent tab on a target is `target-agent-tab`'s (`target_agent_tab_acceptance.rs`).
//!
//! PRD: docs/ft/daemon/session-liveness.md (A target session's tabs) (`#session-liveness` 6/8)
//!
//! The session's target resolves through an in-memory `TabTargetShell`
//! (`target_tab_support::fake_target_shells`), injected with
//! `DaemonSessionHost::with_tab_target_shell`, to a machine on this host standing for the target.
//! Everything else is the daemon's own: the terminal RPCs, the roster, the target tab's launch.

mod support;
mod target_tab_support;

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::LaunchNode;
use tddy_rpc::Code;

use target_tab_support::fake_target_shells::{
    target_shells_reaching_a_machine, target_shells_reaching_an_unreachable_machine,
    target_shells_refusing, UNREACHABLE,
};
use target_tab_support::{
    a_target_session_whose_shells_resolve_through, kinds_of, where_the_login_shell_started,
    wire_kinds_of,
};

#[tokio::test]
async fn a_target_shell_tab_is_recorded_as_target_then_shell() {
    // Given a target session whose target is a reachable machine
    let (target_shells, machine) = target_shells_reaching_a_machine();
    let daemon = a_target_session_whose_shells_resolve_through(target_shells);

    // When a shell tab is opened "On target"
    let tab = daemon
        .opens_a_tab_on_target("bash")
        .await
        .expect("a shell tab starts on the target");

    // Then the session's target was resolved as its owner, the target's login shell runs in the
    // session's worktree there ...
    assert_eq!(
        daemon.target_shells.asked(),
        daemon.the_sessions_owner_asked(1)
    );
    assert_eq!(
        where_the_login_shell_started(&machine).await,
        machine.worktree()
    );
    // ... and the tab is recorded and listed as a shell run on the target
    assert_eq!(
        daemon.roster_entry(&tab).launch,
        vec![LaunchNode::target(), LaunchNode::shell()]
    );
    let listed = daemon.listed_terminal(&tab).await;
    assert_eq!(
        (listed.kind.as_str(), wire_kinds_of(&listed)),
        ("bash", vec!["target", "shell"])
    );
}

#[tokio::test]
async fn an_on_this_host_tab_of_a_target_session_is_a_plain_shell() {
    // Given a target session whose target is a reachable machine
    let (target_shells, machine) = target_shells_reaching_a_machine();
    let daemon = a_target_session_whose_shells_resolve_through(target_shells);

    // When a shell tab is opened "On this host"
    let tab = daemon
        .opens_a_shell_on_this_host()
        .await
        .expect("a shell tab starts on this host");

    // Then it runs on this host in the session's own directory — its worktree is on the target —
    // and is recorded and listed as a plain shell, and nothing was asked of the target
    assert_eq!(daemon.directory_of(&tab).await, daemon.session_dir());
    assert_eq!(kinds_of(&daemon.roster_entry(&tab)), vec!["shell"]);
    let listed = daemon.listed_terminal(&tab).await;
    assert_eq!(
        (listed.kind.as_str(), wire_kinds_of(&listed)),
        ("bash", vec!["shell"])
    );
    assert_eq!(daemon.target_shells.asked(), Vec::new());
    assert_eq!(machine.interactive_commands(), Vec::<String>::new());
}

#[tokio::test]
async fn a_shell_tab_on_a_target_that_cannot_be_reached_is_unavailable_with_the_transports_words() {
    // Given a target session whose target resolves, but whose transport cannot reach it
    let (target_shells, machine) = target_shells_reaching_an_unreachable_machine();
    let daemon = a_target_session_whose_shells_resolve_through(target_shells);

    // When a shell tab is opened "On target"
    let refusal = daemon
        .opens_a_tab_on_target("bash")
        .await
        .expect_err("a tab on an unreachable target fails");

    // Then the target, not the request, is at fault: UNAVAILABLE carrying what the transport said,
    // nothing was opened there, and nothing is recorded
    assert_eq!(refusal.code(), Code::Unavailable);
    assert!(
        refusal.message().contains(UNREACHABLE),
        "the refusal carries the transport's words: {}",
        refusal.message()
    );
    assert_eq!(machine.interactive_commands(), Vec::<String>::new());
    assert_eq!(daemon.roster().tabs, Vec::new());
}

#[tokio::test]
async fn a_shell_tab_on_a_target_that_cannot_be_resolved_stays_a_failed_precondition() {
    // Given a target session whose owner's vault is locked, so its target cannot be resolved
    let locked = "the vault is locked: unlock it to reach this session's target";
    let daemon = a_target_session_whose_shells_resolve_through(target_shells_refusing(
        tddy_rpc::Status::failed_precondition(locked),
    ));

    // When a shell tab is opened "On target"
    let refusal = daemon
        .opens_a_tab_on_target("bash")
        .await
        .expect_err("a tab on an unresolvable target fails");

    // Then it is refused as a precondition the caller can meet, saying why, and nothing is recorded
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert_eq!(refusal.message(), locked);
    assert_eq!(daemon.roster().tabs, Vec::new());
}
