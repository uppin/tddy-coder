//! Acceptance tests: the conversation tools the MCP server advertises follow the live roster.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md (AC14, AC16)
//! Changeset: docs/dev/1-WIP/2026-08-16-session-agent-roster.md
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

use std::sync::atomic::{AtomicU64, Ordering};

use serial_test::serial;
use tddy_service::proto::connection::{SessionAgentEntry, SessionAgentRoster};
use tddy_tools::server::PermissionServer;
use tddy_tools::session_agents::session_agent_roster;

/// The ACP-shaped conversation tools, in the order `tools/list` reports them (by name).
const CONVERSATION_TOOLS: [&str; 4] = [
    "subagent_cancel",
    "subagent_list",
    "subagent_new_session",
    "subagent_prompt",
];

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
