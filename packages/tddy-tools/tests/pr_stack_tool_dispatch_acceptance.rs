//! Acceptance: every PR-stack tool the registry names is actually reachable.
//!
//! A tool has to be registered in independent places to work: the registry in `tddy-tool-engine`
//! (`WORK_QUEUE_TOOL_NAMES` and `WORK_ITEM_GIT_TOOL_NAMES`), the definitions `tddy-tools` advertises over MCP,
//! and the `call_tool_by_name` dispatch mirror used by `tddy-tools call-tool` and the web Inspector.
//! No single owner enforces the agreement, so a tool present in the registry but not in the other two
//! is silently unreachable — named but not advertised, or advertised but not dispatchable.
//!
//! The first two tests are that owner. They are deliberately written over the registry rather than
//! over a hardcoded list of names, so a tool added to the registry without being wired up fails here
//! rather than at an operator's prompt. Both express their expectation as "nothing is missing", which
//! is empty by construction if the registry stops looking the way `registered_tool_names` expects —
//! hence the cardinality guard there, and the count assertion on the dispatch loop.
//!
//! Since `#orchestrators` 7/11 the plan and read tools are not methods of this crate at all: they
//! are the daemon's exec tools, addressed by `queue_id`
//! ([`tddy_tool_engine::work_queue_tools`]). The third test pins that move — registered only where there
//! is a daemon to dispatch to, advertised only when the daemon admits them for the session, and a
//! call by name forwarded to the daemon rather than run against `TDDY_SESSION_DIR` here.
//!
//! PRD: docs/ft/coder/pr-stacking.md § PR-management tools.
//! Changeset: docs/dev/changesets/2026-07-30-pr-stack-full-control.md.
//! Changeset: docs/dev/1-WIP/2026-11-10-stack-plan-tools.md.

use serial_test::serial;
use tddy_tool_engine::work_item_annotation_tools::WORK_ITEM_ANNOTATION_TOOL_NAMES;
use tddy_tool_engine::work_item_git_tools::WORK_ITEM_GIT_TOOL_NAMES;
use tddy_tool_engine::work_item_link_tools::WORK_ITEM_LINK_TOOL_NAMES;
use tddy_tool_engine::work_queue_tools::WORK_QUEUE_TOOL_NAMES;
use tddy_tools::server::{PermissionServer, UNKNOWN_TOOL_REJECTION};

/// The registry's tool names, bare: the router and the dispatch mirror both key off the bare tool
/// name.
///
/// Fails when the registry yields fewer names than its two lists hold, because both tests below
/// express their expectation as "nothing is missing": a gather that silently dropped every name
/// would leave them comparing two empty collections and passing while checking nothing.
fn registered_tool_names() -> Vec<String> {
    let names: Vec<String> = WORK_QUEUE_TOOL_NAMES
        .iter()
        .chain(WORK_ITEM_LINK_TOOL_NAMES.iter())
        .chain(WORK_ITEM_ANNOTATION_TOOL_NAMES.iter())
        .chain(WORK_ITEM_GIT_TOOL_NAMES.iter())
        .map(|name| name.to_string())
        .collect();
    assert_eq!(
        names.len(),
        WORK_QUEUE_TOOL_NAMES.len()
            + WORK_ITEM_LINK_TOOL_NAMES.len()
            + WORK_ITEM_ANNOTATION_TOOL_NAMES.len()
            + WORK_ITEM_GIT_TOOL_NAMES.len(),
        "every registered PR-stack name must be gathered, or these tests have nothing left to \
         check; registry: {names:?}"
    );
    names
}

#[test]
#[serial]
fn every_registered_pr_stack_tool_is_advertised_in_the_mcp_tool_definitions() {
    // Given — the tools the registry names, and the definitions the MCP server advertises
    let expected = registered_tool_names();
    let advertised: Vec<String> = PermissionServer::advertised_tool_defs()
        .into_iter()
        .map(|def| def.name)
        .collect();

    // When — every registered name is looked for among them
    let missing: Vec<&String> = expected
        .iter()
        .filter(|name| !advertised.contains(name))
        .collect();

    // Then — a registered tool the server never advertises is one the registry names and the agent
    // then cannot see
    assert_eq!(
        missing,
        Vec::<&String>::new(),
        "these registered PR-stack tools are not advertised over MCP; advertised: {advertised:?}"
    );
}

#[tokio::test]
#[serial]
async fn every_registered_pr_stack_tool_is_dispatchable_by_name() {
    // Given — the same registry, and a server with no session in scope
    let server = PermissionServer::new();
    let expected = registered_tool_names();

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
        "every registered tool must have been dispatched; gathered: {outcomes:?}"
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
        "these registered PR-stack tools are missing from the call_tool_by_name dispatch"
    );
}

fn clear_session_tool_transport_env() {
    std::env::remove_var("TDDY_SANDBOX_TOOL_IPC");
    std::env::remove_var("TDDY_REMOTE_SESSION_ID");
    std::env::remove_var("TDDY_REMOTE_DAEMON_URL");
}

/// The stack tools among `tools`, sorted: a router lists its tools in no promised order.
fn work_queue_tools_among(tools: Vec<rmcp::model::Tool>) -> Vec<String> {
    let mut names: Vec<String> = tools
        .into_iter()
        .map(|tool| tool.name.to_string())
        .filter(|name| WORK_QUEUE_TOOL_NAMES.contains(&name.as_str()))
        .collect();
    names.sort();
    names
}

#[tokio::test]
#[serial]
async fn the_plan_and_read_stack_tools_are_dispatched_to_the_daemon() {
    // Given a server with no daemon to dispatch to, and one inside a session's jail
    clear_session_tool_transport_env();
    let without_a_daemon = PermissionServer::new();
    std::env::set_var(
        "TDDY_SANDBOX_TOOL_IPC",
        "/tmp/tddy-stack-tools-dispatch.sock",
    );
    let with_a_daemon = PermissionServer::new();
    clear_session_tool_transport_env();
    let mut every_stack_tool: Vec<String> = WORK_QUEUE_TOOL_NAMES
        .iter()
        .map(|n| n.to_string())
        .collect();
    every_stack_tool.sort();

    // When each is asked what it offers, and the one with no daemon is asked to run each stack tool
    let offered_without_a_daemon = work_queue_tools_among(without_a_daemon.advertised_tools());
    let offered_when_admitted =
        work_queue_tools_among(with_a_daemon.advertised_tools_admitting(&every_stack_tool));
    let offered_when_not_admitted =
        work_queue_tools_among(with_a_daemon.advertised_tools_admitting(&[]));
    let mut not_forwarded = Vec::new();
    for name in WORK_QUEUE_TOOL_NAMES {
        let outcome = without_a_daemon
            .call_tool_by_name(name, serde_json::json!({}))
            .await;
        if !matches!(&outcome, Ok(answer) if answer.contains("remote toolset not configured")) {
            not_forwarded.push((name, outcome));
        }
    }

    // Then none is this crate's own: with no daemon none is offered, with one each is offered
    // exactly when the daemon admits it, and a call by name goes to the session-tool client — which,
    // with no daemon configured, says so — rather than to TDDY_SESSION_DIR
    assert_eq!(offered_without_a_daemon, Vec::<String>::new());
    assert_eq!(offered_when_admitted, every_stack_tool);
    assert_eq!(offered_when_not_admitted, Vec::<String>::new());
    assert_eq!(not_forwarded, Vec::new());
}
