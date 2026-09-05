//! Acceptance: every PR-stack tool the orchestrator is allowed to call is actually reachable.
//!
//! A tool has to be registered in three independent places to work: `PR_STACK_TOOL_NAMES` (which
//! becomes `--allowedTools` for the `orchestrate` goal), the `#[tool_router]` block in `tddy-tools`
//! (which is what MCP advertises), and the `call_tool_by_name` dispatch mirror used by
//! `tddy-tools call-tool` and the web Inspector. No single owner enforces the agreement, so a tool
//! present in two of the three is silently unreachable — allowlisted and advertised but not
//! dispatchable, or dispatchable but never offered to the agent.
//!
//! These two tests are that owner. They are deliberately written over the allowlist rather than over
//! a hardcoded list of names, so a tool added to the allowlist without being wired up fails here
//! rather than at an operator's prompt. Both express their expectation as "nothing is missing", which
//! is empty by construction if the allowlist stops looking the way `allowlisted_tool_names` expects —
//! hence the cardinality guard there, and the count assertion on the dispatch loop.
//!
//! PRD: docs/ft/coder/pr-stacking.md § PR-management tools.
//! Changeset: docs/dev/changesets/2026-07-30-pr-stack-full-control.md.

use tddy_tools::server::{PermissionServer, UNKNOWN_TOOL_REJECTION};
use tddy_workflow_recipes::pr_stack::PR_STACK_TOOL_NAMES;

/// The prefix every allowlisted name carries: `--allowedTools` is fully qualified, while the router
/// and the dispatch mirror both key off the bare tool name.
const MCP_PREFIX: &str = "mcp__tddy-tools__";

/// The allowlist as bare tool names.
///
/// Fails when the allowlist holds a name this prefix does not describe, because both tests below
/// express their expectation as "nothing is missing": a strip that silently dropped every name would
/// leave them comparing two empty collections and passing while checking nothing.
fn allowlisted_tool_names() -> Vec<String> {
    let names: Vec<String> = PR_STACK_TOOL_NAMES
        .iter()
        .filter_map(|name| name.strip_prefix(MCP_PREFIX))
        .map(str::to_string)
        .collect();
    assert_eq!(
        names.len(),
        PR_STACK_TOOL_NAMES.len(),
        "every allowlisted PR-stack name must be an `{MCP_PREFIX}` tool, or these tests have \
         nothing left to check; allowlist: {PR_STACK_TOOL_NAMES:?}"
    );
    names
}

#[test]
fn every_allowlisted_pr_stack_tool_is_advertised_in_the_mcp_tool_definitions() {
    // Given — the tools the orchestrate goal is started with, and the definitions the MCP server
    // advertises
    let expected = allowlisted_tool_names();
    let advertised: Vec<String> = PermissionServer::advertised_tool_defs()
        .into_iter()
        .map(|def| def.name)
        .collect();

    // When — every allowlisted name is looked for among them
    let missing: Vec<&String> = expected
        .iter()
        .filter(|name| !advertised.contains(name))
        .collect();

    // Then — an allowlisted tool the server never advertises is one the agent is told it may call
    // and then cannot see
    assert_eq!(
        missing,
        Vec::<&String>::new(),
        "these allowlisted PR-stack tools are not advertised over MCP; advertised: {advertised:?}"
    );
}

#[tokio::test]
async fn every_allowlisted_pr_stack_tool_is_dispatchable_by_name() {
    // Given — the same allowlist, and a server with no session in scope
    let server = PermissionServer::new();
    let expected = allowlisted_tool_names();

    // When — each name is dispatched with empty arguments. Without `TDDY_SESSION_DIR` a wired-up
    // tool reports its own refusal (an `Ok` JSON `{"error": …}`) or an argument-parse `Err`; only an
    // unregistered name produces `UNKNOWN_TOOL_REJECTION`. That distinction is the assertion — this
    // test pins reachability, not behaviour.
    let mut outcomes: Vec<(String, Result<String, String>)> = Vec::new();
    for name in &expected {
        let outcome = server.call_tool_by_name(name, serde_json::json!({})).await;
        outcomes.push((name.clone(), outcome));
    }

    // Then — every name was dispatched, and none of them was rejected as unknown
    assert_eq!(
        outcomes.len(),
        expected.len(),
        "every allowlisted tool must have been dispatched; gathered: {outcomes:?}"
    );
    let unreachable: Vec<&String> = outcomes
        .iter()
        .filter(|(_, outcome)| {
            matches!(outcome, Err(reason) if reason.contains(UNKNOWN_TOOL_REJECTION))
        })
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        unreachable,
        Vec::<&String>::new(),
        "these allowlisted PR-stack tools are missing from the call_tool_by_name dispatch"
    );
}
