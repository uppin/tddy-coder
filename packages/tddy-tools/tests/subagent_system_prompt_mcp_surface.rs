//! Acceptance tests: the MCP surface lets the agent opening a conversation set its system prompt,
//! and tells it that it can.
//!
//! The library half — that an override replaces the def's prompt for one conversation and nothing
//! else — is covered in `tddy-discovery`
//! (`tests/subagent_system_prompt_override.rs`). What is wrong-able *here* is whether the
//! capability reaches the main agent at all: a parameter absent from the advertised schema is a
//! parameter no model will ever send, however faithfully the layer beneath it behaves.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use serial_test::serial;
use tddy_service::proto::session_agents_svc::{SessionAgentEntry, SessionAgentRoster};
use tddy_tools::server::PermissionServer;
use tddy_tools::session_agents::session_agent_roster;

const AN_ATTACHED_AGENT: &str = "FastContext@LT-R1VXTH2V6H-1790497976955";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// The next roster revision to publish. The registry is process-wide and only moves forward, so a
/// literal revision would make these tests depend on the order they run in.
fn next_rev() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_REV: AtomicU64 = AtomicU64::new(1);
    NEXT_REV.fetch_add(1, Ordering::SeqCst)
}

fn an_entry(agent_id: &str) -> SessionAgentEntry {
    let (name, daemon) = agent_id
        .split_once('@')
        .expect("builder was given a qualified agent id");
    SessionAgentEntry {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        daemon_instance_id: daemon.to_string(),
        label: format!("{name} (local)"),
        model: "fastcontext-tools-32k:latest".to_string(),
        replaces: Vec::new(),
        tools: vec!["Read".to_string(), "Glob".to_string(), "Grep".to_string()],
        codebase_session_id: String::new(),
        clone_state: 1, // AGENT_CLONE_STATE_LOCAL
        clone_error: String::new(),
        status: 0, // SESSION_AGENT_STATUS_UNSPECIFIED
        last_activity: None,
    }
}

fn a_session_with_one_attached_agent() {
    session_agent_roster().apply_snapshot(SessionAgentRoster {
        session_id: "1790497976955-roster".to_string(),
        rev: next_rev(),
        agents: vec![an_entry(AN_ATTACHED_AGENT)],
    });
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// The advertised `subagent_new_session` input schema, as `tools/list` reports it.
fn the_advertised_new_session_schema() -> serde_json::Value {
    let tool = PermissionServer::new()
        .advertised_tools()
        .into_iter()
        .find(|tool| tool.name == "subagent_new_session")
        .expect("subagent_new_session must be advertised while an agent is attached");
    serde_json::Value::Object((*tool.input_schema).clone())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A capability the schema does not mention is one no model will use.
#[test]
#[serial]
fn opening_a_conversation_advertises_a_system_prompt_the_caller_may_set() {
    // Given a session with one agent attached
    a_session_with_one_attached_agent();

    // When the advertised schema is read
    let schema = the_advertised_new_session_schema();

    // Then it offers a systemPrompt
    assert_eq!(
        schema["properties"]["systemPrompt"]["type"],
        serde_json::json!("string")
    );
}

/// Overriding is a choice, not a requirement: a caller that omits it gets the def's own prompt,
/// so the parameter must not join `agent` in the required set.
#[test]
#[serial]
fn the_system_prompt_is_optional_so_an_existing_caller_keeps_the_defs_prompt() {
    // Given a session with one agent attached
    a_session_with_one_attached_agent();

    // When the advertised schema is read
    let schema = the_advertised_new_session_schema();

    // Then systemPrompt is offered, and offered as optional
    //
    // Both halves are asserted here on purpose: a schema that never declares the parameter would
    // also leave it out of `required`, so checking only the second half is a test that passes
    // before the feature exists.
    let offered: Vec<&str> = schema["properties"]
        .as_object()
        .expect("an input schema declares its properties")
        .keys()
        .map(String::as_str)
        .collect();
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|values| values.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    assert!(
        offered.contains(&"systemPrompt") && !required.contains(&"systemPrompt"),
        "an omitted override must leave the def's prompt in place, which needs the parameter to \
         exist and to be optional; offered {offered:?}, required {required:?}"
    );
}

/// The description is the only place a model learns what the parameter *does*. "Replaces" is the
/// load-bearing word: a caller that thinks it appends will write half a prompt.
#[test]
#[serial]
fn the_system_prompt_description_says_it_replaces_the_defs_own_prompt() {
    // Given a session with one agent attached
    a_session_with_one_attached_agent();

    // When the advertised schema is read
    let schema = the_advertised_new_session_schema();

    // Then the description says what it does to the def's prompt
    let description = schema["properties"]["systemPrompt"]["description"]
        .as_str()
        .expect("the parameter must describe itself")
        .to_lowercase();
    assert!(
        description.contains("replace"),
        "a caller must learn the override replaces rather than appends; got: {description}"
    );
}
