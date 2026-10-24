//! `ListSubagents` offers headless harnesses as coding agents, with their transport and policy.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md

use tddy_daemon_rpc::test_util::test_service;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{CatalogService, ListSubagentsRequest, SubagentInfo};
use tddy_service::proto::types::AgentKind;

async fn rows_offered_for(profiles: &[(&str, &str)]) -> Vec<SubagentInfo> {
    let home = tempfile::tempdir().expect("home");
    let agents = home.path().join("agents");
    std::fs::create_dir_all(&agents).expect("agents dir");
    for (name, harness) in profiles {
        std::fs::write(
            agents.join(format!("{name}.yaml")),
            format!("name: {name}\nharness: {harness}\nmodel: m\n"),
        )
        .expect("profile");
    }
    let service = test_service(home.path().to_path_buf());
    service
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents")
        .into_inner()
        .subagents
}

#[tokio::test]
async fn headless_harnesses_are_listed_with_their_transport_and_permission_policy() {
    // When
    let rows = rows_offered_for(&[("plain-claude", "claude"), ("plain-codex", "codex")]).await;

    // Then
    let claude = rows
        .iter()
        .find(|row| row.name == "plain-claude")
        .expect("claude row");
    assert_eq!(claude.kind, AgentKind::CodingAgent as i32);
    assert_eq!(claude.transport, "headless");
    assert_eq!(claude.permission_policy, "asks_caller");
    let codex = rows
        .iter()
        .find(|row| row.name == "plain-codex")
        .expect("codex row");
    assert!(
        codex.permission_policy.starts_with("fixed:"),
        "{}",
        codex.permission_policy
    );
}

#[tokio::test]
async fn a_harness_offered_both_ways_is_listed_once_per_transport() {
    // When
    let rows = rows_offered_for(&[("claude-p", "claude"), ("claude-a", "claude-acp")]).await;

    // Then
    let transports: Vec<(&str, &str)> = rows
        .iter()
        .filter(|row| row.kind == AgentKind::CodingAgent as i32)
        .map(|row| (row.name.as_str(), row.transport.as_str()))
        .collect();
    assert!(
        transports.contains(&("claude-p", "headless")),
        "{transports:?}"
    );
    assert!(transports.contains(&("claude-a", "acp")), "{transports:?}");
}
