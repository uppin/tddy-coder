//! The one CLI launch's agent resolution (`#agent-tools 17/19`): which CLI session type a start,
//! a re-attach and a spawned child run, read from the host's installed coding agents and never
//! from an agent's name.

use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_rpc::{Code, Status};
use tddy_worktree_service::branch_intent::{BlankNewBranchName, BranchIntentPolicy};

use super::*;

/// The built-in coding agents on a host whose binaries are never resolved: these tests read which
/// agent a session type is, not where its binary lives.
fn the_builtin_coding_agents() -> HostCodingAgents {
    let no_overrides = tempfile::tempdir().expect("an empty tools dir");
    HostCodingAgents::new(
        tddy_coding_agent_specs::load_agent_registry(no_overrides.path())
            .expect("the built-in coding agents load"),
        Arc::new(tddy_actions::tool::ProcessToolHost),
    )
}

/// `(name, spec_id)` of a resolved session type — the two fields the launch reads.
fn named(kind: CliSessionType) -> (&'static str, &'static str) {
    (kind.name, kind.spec_id)
}

/// The code and message of a refusal.
fn refusal(status: Status) -> (Code, String) {
    (status.code, status.message)
}

/// What a branch policy does with a blank new branch, and the prefix of a generated one.
fn branch_behaviour(policy: BranchIntentPolicy) -> (&'static str, BlankNewBranchName) {
    (policy.generated_branch_prefix, policy.blank_new_branch_name)
}

// ── cli_launcher ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cli_launcher_resolves_every_builtin_cli_agent_to_its_session_type_and_spec() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let resolved: Vec<Option<(&str, &str)>> = [
        "claude-cli",
        "cursor-cli",
        "opencode-cli",
        "muse-cli",
        "warp-cli",
    ]
    .iter()
    .map(|session_type| {
        cli_launcher(Some(&agents), session_type)
            .expect("a built-in CLI agent's session type resolves")
            .map(named)
    })
    .collect();

    // Then
    assert_eq!(
        resolved,
        vec![
            Some(("claude-cli", "claude")),
            Some(("cursor-cli", "cursor")),
            Some(("opencode-cli", "opencode")),
            Some(("muse-cli", "muse")),
            Some(("warp-cli", "warp")),
        ]
    );
}

#[test]
fn cli_launcher_is_none_for_a_tool_and_a_workspace_session() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let tool = cli_launcher(Some(&agents), "tool").map(|kind| kind.map(named));
    let workspace = cli_launcher(Some(&agents), "workspace").map(|kind| kind.map(named));

    // Then
    assert_eq!((tool.ok(), workspace.ok()), (Some(None), Some(None)));
}

#[test]
fn cli_launcher_refuses_a_cli_session_type_on_a_host_without_coding_agents() {
    // When
    let refused = cli_launcher(None, "cursor-cli").map(|kind| kind.map(named));

    // Then
    assert_eq!(
        refused.map_err(refusal),
        Err((
            Code::FailedPrecondition,
            "this host has no coding agents installed: a cursor-cli session is launched from its \
             tool definition"
                .to_string()
        ))
    );
}

// ── recorded_cli_session_type (re-attach) ─────────────────────────────────────────────────────

#[test]
fn a_record_without_a_session_type_reattaches_as_no_agent() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let recorded = recorded_cli_session_type(Some(&agents), None).map(|kind| kind.map(named));

    // Then
    assert_eq!(recorded.map_err(refusal), Ok(None));
}

#[test]
fn a_claude_cli_record_reattaches_as_the_installed_claude_agent() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let recorded =
        recorded_cli_session_type(Some(&agents), Some("claude-cli")).map(|kind| kind.map(named));

    // Then
    assert_eq!(
        recorded.map_err(refusal),
        Ok(Some(("claude-cli", "claude")))
    );
}

#[test]
fn a_workspace_record_reattaches_as_no_agent() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let recorded =
        recorded_cli_session_type(Some(&agents), Some("workspace")).map(|kind| kind.map(named));

    // Then
    assert_eq!(recorded.map_err(refusal), Ok(None));
}

#[test]
fn a_record_on_a_host_without_coding_agents_is_refused_failed_precondition() {
    // When
    let recorded = recorded_cli_session_type(None, Some("claude-cli")).map(|kind| kind.map(named));

    // Then
    assert_eq!(
        recorded.map_err(refusal),
        Err((
            Code::FailedPrecondition,
            "this host has no coding agents installed: a claude-cli session is launched from its \
             tool definition"
                .to_string()
        ))
    );
}

// ── child_session_type (conversation and pr-stack children) ───────────────────────────────────

#[test]
fn a_child_of_a_cursor_cli_session_runs_cursor_cli() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let child = child_session_type(Some(&agents), Some("cursor-cli")).map(named);

    // Then
    assert_eq!(child.map_err(refusal), Ok(("cursor-cli", "cursor")));
}

#[test]
fn a_child_of_an_opencode_cli_session_runs_opencode_cli() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let child = child_session_type(Some(&agents), Some("opencode-cli")).map(named);

    // Then
    assert_eq!(child.map_err(refusal), Ok(("opencode-cli", "opencode")));
}

#[test]
fn a_child_of_a_parent_that_records_no_session_type_is_refused_failed_precondition() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let child = child_session_type(Some(&agents), None).map(named);

    // Then — never a default agent
    assert_eq!(
        child.map_err(|status| status.code),
        Err(Code::FailedPrecondition)
    );
}

#[test]
fn a_child_of_a_tool_session_is_refused_failed_precondition() {
    // Given
    let agents = the_builtin_coding_agents();

    // When
    let child = child_session_type(Some(&agents), Some("tool")).map(named);

    // Then
    assert_eq!(
        child.map_err(|status| status.code),
        Err(Code::FailedPrecondition)
    );
}

#[test]
fn a_child_on_a_host_without_coding_agents_is_refused_failed_precondition() {
    // When
    let child = child_session_type(None, Some("claude-cli")).map(named);

    // Then
    assert_eq!(
        child.map_err(refusal),
        Err((
            Code::FailedPrecondition,
            "this host has no coding agents installed: a claude-cli session is launched from its \
             tool definition"
                .to_string()
        ))
    );
}

// ── cli_branch_intent ────────────────────────────────────────────────────────────────────────

#[test]
fn the_spec_branch_intent_of_the_builtin_agents_follows_their_branch_name_required_capability() {
    // Given the built-in claude (which declares branch_name_required) and cursor (which does not)
    let agents = the_builtin_coding_agents();
    let intent_of = |session_type: &str| {
        let kind = cli_session_type(&agents, session_type).expect("a built-in session type");
        let spec = agents
            .agents()
            .for_session_type(session_type)
            .expect("a built-in spec");
        branch_behaviour(cli_branch_intent(kind, spec))
    };

    // When their intents are read from the specs
    let from_specs = (intent_of("claude-cli"), intent_of("cursor-cli"));

    // Then claude refuses a blank new branch name and cursor generates one, each under its session type
    assert_eq!(
        from_specs,
        (
            ("claude-cli", BlankNewBranchName::Refuse),
            ("cursor-cli", BlankNewBranchName::Generate),
        )
    );
}
