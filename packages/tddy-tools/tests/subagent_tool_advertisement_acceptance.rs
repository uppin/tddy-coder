//! Acceptance tests: the tools the MCP server advertises follow the live roster — both the
//! conversation tools an attached agent makes reachable, and the exec tools it takes away.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC14, AC16)
//!
//! `subagent_mcp_acceptance.rs` covers what a *spawn* looks like — a session started with an agent
//! advertises the conversation tools, one started without does not — by starting a fresh process
//! per case. What that cannot show is the case this feature exists for: the roster changing
//! underneath a process that is already running, so an agent attached at minute forty becomes
//! callable without a restart.
//!
//! Frames are pushed straight into the process-wide roster rather than served over a socket. The
//! transport is covered where it is implemented; what is wrong-able here is whether the advertised
//! tool list is derived from the roster at all, and a real stream would only make that
//! non-deterministic.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use pretty_assertions::assert_eq;
use serial_test::serial;
use tddy_service::proto::connection::{SessionAgentEntry, SessionAgentRoster};
use tddy_tools::server::{exec_tool_catalog, PermissionServer};
use tddy_tools::session_agents::session_agent_roster;

/// The ACP-shaped conversation tools, in the order `tools/list` reports them (by name).
const CONVERSATION_TOOLS: [&str; 4] = [
    "subagent_cancel",
    "subagent_list",
    "subagent_new_session",
    "subagent_prompt",
];

/// The env var that tells the server a session-tool transport is reachable, and so makes it merge
/// the exec-tool catalog (`Read`/`Grep`/`Shell`/…) into its router — the tools a `replaces` list
/// takes over.
const IPC_SOCKET_ENV: &str = "TDDY_SANDBOX_TOOL_IPC";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The next roster revision to publish.
///
/// The registry is process-wide and only ever moves forward, so a test that published a literal
/// revision would be ignored whenever another test had already published a higher one — making
/// these tests depend on the order they run in. Each takes the next revision instead.
fn next_rev() -> u64 {
    static NEXT_REV: AtomicU64 = AtomicU64::new(1);
    NEXT_REV.fetch_add(1, Ordering::SeqCst)
}

/// A roster entry as the daemon publishes it.
fn an_entry(agent_id: &str) -> SessionAgentEntry {
    let (name, daemon) = agent_id
        .split_once('@')
        .expect("builder was given a qualified agent id");
    SessionAgentEntry {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        daemon_instance_id: daemon.to_string(),
        label: format!("{name} (local)"),
        model: "qwen2.5-coder:7b".to_string(),
        replaces: Vec::new(),
        tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
        codebase_session_id: String::new(),
        clone_state: 1, // AGENT_CLONE_STATE_LOCAL
        clone_error: String::new(),
    }
}

/// A roster entry for an agent that has taken over `replaced` from the main agent.
fn an_entry_replacing(agent_id: &str, replaced: &[&str]) -> SessionAgentEntry {
    SessionAgentEntry {
        replaces: replaced.iter().map(|tool| tool.to_string()).collect(),
        ..an_entry(agent_id)
    }
}

/// Publish a roster to the process this test runs in, at the next revision.
fn publish(agents: Vec<SessionAgentEntry>) {
    session_agent_roster().apply_snapshot(SessionAgentRoster {
        session_id: "1780828020298-roster".to_string(),
        rev: next_rev(),
        agents,
    });
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// The conversation tools a freshly-built server advertises — the exact set `tools/list` reports.
fn advertised_conversation_tools() -> Vec<String> {
    PermissionServer::new()
        .tool_names()
        .into_iter()
        .filter(|name| CONVERSATION_TOOLS.contains(&name.as_str()))
        .collect()
}

/// The exec-catalog tools a freshly-built server does **not** advertise, sorted.
///
/// Built with a session-tool transport configured, as the jail has, since that is the only condition
/// under which the exec catalog is in the router at all. The var is process-global, so this is for
/// `#[serial]` tests only.
fn exec_tools_withheld_from_the_main_agent() -> Vec<String> {
    std::env::set_var(IPC_SOCKET_ENV, "/tmp/tddy-roster-advertisement-ipc.sock");
    let advertised: HashSet<String> = PermissionServer::new().tool_names().into_iter().collect();
    std::env::remove_var(IPC_SOCKET_ENV);

    let mut withheld: Vec<String> = exec_tool_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .filter(|name| !advertised.contains(name))
        .collect();
    withheld.sort();
    withheld
}

/// The exec-catalog tools a direct call is refused, sorted — the roster's other enforcement point.
fn exec_tools_refused_on_a_direct_call() -> Vec<String> {
    let mut refused: Vec<String> = exec_tool_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .filter(|name| session_agent_roster().check_tool_available(name).is_err())
        .collect();
    refused.sort();
    refused
}

// ---------------------------------------------------------------------------
// AC14/AC16 — advertisement follows the roster, in-process
// ---------------------------------------------------------------------------

/// The headline of live attach, seen from the main agent's tool list: an agent attached while the
/// process runs makes the conversation tools appear, so the `tools/list_changed` the roster emits
/// has something new to report.
#[tokio::test]
#[serial]
async fn advertises_the_conversation_tools_once_a_roster_frame_attaches_an_agent() {
    // Given
    publish(vec![]);

    // When
    publish(vec![an_entry("explorer@ws-01")]);

    // Then
    assert_eq!(advertised_conversation_tools(), CONVERSATION_TOOLS.to_vec());
}

/// And the other direction: with the last agent detached there is nobody to open a conversation
/// with, so the tools go rather than staying as four that can only refuse.
#[tokio::test]
#[serial]
async fn withdraws_the_conversation_tools_once_the_last_agent_is_detached() {
    // Given
    publish(vec![an_entry("explorer@ws-01")]);

    // When
    publish(vec![]);

    // Then
    assert_eq!(advertised_conversation_tools(), Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// AC14/AC16 — tool replacement follows the roster, in-process
// ---------------------------------------------------------------------------

/// The point of selecting a specialized agent: the tools it owns leave the main agent's catalog, so
/// the main agent has to go through the agent instead of doing the work itself. Enforcement at call
/// time alone is not enough — a tool that is still advertised is a tool the main agent will keep
/// reaching for, and every reach is a refusal it has to recover from mid-turn.
#[tokio::test]
#[serial]
async fn stops_advertising_the_tools_the_attached_agent_replaces() {
    // Given
    publish(vec![]);

    // When
    publish(vec![an_entry_replacing(
        "fastcontext@ws-01",
        &["Grep", "Glob"],
    )]);

    // Then
    assert_eq!(
        exec_tools_withheld_from_the_main_agent(),
        vec!["Glob".to_string(), "Grep".to_string()]
    );
}

/// And back, on detach: the agent that took the tools over is gone, so withholding them would leave
/// the session with neither the tool nor a replacement for it.
#[tokio::test]
#[serial]
async fn advertises_them_again_once_the_agent_holding_them_detaches() {
    // Given
    publish(vec![an_entry_replacing(
        "fastcontext@ws-01",
        &["Grep", "Glob"],
    )]);

    // When
    publish(vec![]);

    // Then
    assert_eq!(
        exec_tools_withheld_from_the_main_agent(),
        Vec::<String>::new()
    );
}

/// The two enforcement points have to agree. They are read at different moments — the catalog when
/// the agent lists tools, the check when it calls one — and a disagreement is invisible from either
/// side: a tool advertised and then refused looks like a broken tool, and one withheld but callable
/// is a withdrawal that only appears to have happened.
#[tokio::test]
#[serial]
async fn withholds_from_the_catalog_exactly_what_a_direct_call_is_refused() {
    // Given
    publish(vec![]);

    // When
    publish(vec![
        an_entry_replacing("fastcontext@ws-01", &["Grep"]),
        an_entry_replacing("librarian@ws-02", &["Read", "Glob"]),
    ]);

    // Then
    assert_eq!(
        exec_tools_withheld_from_the_main_agent(),
        exec_tools_refused_on_a_direct_call()
    );
}
