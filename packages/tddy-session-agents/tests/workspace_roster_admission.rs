//! A `workspace` session's roster holds only coding agents its own daemon owns.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-remote-codebase.md

use tddy_core::{AgentKindRecord, SessionAgentRecord};
use tddy_rpc::Code;
use tddy_session_agents::workspace_roster::workspace_roster_admits;

const CODEBASE_HOST: &str = "codebase-host";

fn a_record(name: &str, owner: &str, kind: AgentKindRecord) -> SessionAgentRecord {
    SessionAgentRecord {
        agent_id: format!("{name}@{owner}"),
        name: name.to_string(),
        daemon_instance_id: owner.to_string(),
        label: None,
        model: "stub-model".to_string(),
        replaces: Vec::new(),
        tools: Vec::new(),
        codebase_session_id: None,
        kind,
        harness: (kind == AgentKindRecord::CodingAgent).then(|| "codex-acp".to_string()),
        placement: Default::default(),
    }
}

#[test]
fn a_coding_agent_of_the_codebase_daemon_is_admitted() {
    // When
    let admitted = workspace_roster_admits(
        &a_record("codex-fast", CODEBASE_HOST, AgentKindRecord::CodingAgent),
        CODEBASE_HOST,
    );

    // Then
    assert!(admitted.is_ok(), "{admitted:?}");
}

#[test]
fn a_model_agent_is_refused_naming_it() {
    // When
    let refusal = workspace_roster_admits(
        &a_record("explorer", CODEBASE_HOST, AgentKindRecord::Model),
        CODEBASE_HOST,
    )
    .expect_err("a model agent gains nothing from the codebase host");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal.message.contains("explorer@codebase-host"),
        "{}",
        refusal.message
    );
}

#[test]
fn a_coding_agent_of_another_daemon_is_refused_naming_it() {
    // When
    let refusal = workspace_roster_admits(
        &a_record("codex-fast", "laptop", AgentKindRecord::CodingAgent),
        CODEBASE_HOST,
    )
    .expect_err("a third daemon would be multi-hop");

    // Then
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal.message.contains("codex-fast@laptop"),
        "{}",
        refusal.message
    );
}
