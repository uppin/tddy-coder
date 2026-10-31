//! Acceptance: `CatalogService.ListAgents` reports each agent's kind, harness and route support,
//! read from the coding-agent registry (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md` § Acceptance Criteria.
//! Closes `docs/dev/todo/2026-10-22-tool-session-provider-picker-needs-harness-metadata.md`: a
//! picker asks for provider routes only for an agent whose `route_providers` is non-empty.
//!
//! The daemon's `<tddyhome>/tools` is empty, so the registry is the built-in specs.

use std::path::PathBuf;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_rpc::Request;
use tddy_service::proto::catalog::{AgentInfo, CatalogService, ListAgentsRequest};
use tddy_service::proto::types::AgentKind;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

/// A daemon whose `allowed_agents` are `ids`, over a `<tddyhome>` with an empty `tools/`.
fn a_daemon_allowing(ids: &[&str]) -> (tempfile::TempDir, TestDaemon) {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("tools")).unwrap();
    let allowed: String = ids.iter().map(|id| format!("  - id: {id}\n")).collect();
    let config_path = home.path().join("daemon.yaml");
    std::fs::write(
        &config_path,
        format!("users:\n  - github_user: u\n    os_user: u\nallowed_agents:\n{allowed}"),
    )
    .unwrap();
    let config = DaemonConfig::load(&config_path).expect("the config loads");
    let tddy_home: PathBuf = home.path().to_path_buf();
    let sessions_base = tddy_home.clone();
    let daemon = TestDaemon::from_host(DaemonSessionHost::new(
        config.clone(),
        Arc::new(move |_| Some(sessions_base.clone())),
        tddy_home,
        Arc::new(|token| (token == "valid-token").then(|| "u".to_string())),
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        tddy_session_lifecycle::peer_routing::PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager::new()),
    ));
    (home, daemon)
}

/// The row a registry agent is listed as.
fn a_coding_agent(id: &str, route_providers: &[&str]) -> AgentInfo {
    AgentInfo {
        id: id.to_string(),
        label: id.to_string(),
        kind: AgentKind::CodingAgent as i32,
        harness: id.to_string(),
        route_providers: route_providers.iter().map(|p| p.to_string()).collect(),
    }
}

#[tokio::test]
async fn the_agent_catalog_reports_kind_harness_and_route_support_per_agent() {
    // Given — three registry agents and one backend id no spec stands for
    let (_home, daemon) = a_daemon_allowing(&["claude", "cursor", "opencode", "zebra-backend"]);

    // When
    let agents = daemon
        .list_agents(Request::direct(ListAgentsRequest {}))
        .await
        .expect("ListAgents succeeds")
        .into_inner()
        .agents;

    // Then — Claude routes to Anthropic, Cursor to nothing, OpenCode to either OpenAI protocol;
    // the unknown id is a MODEL row with no harness and nothing to route
    assert_eq!(
        agents,
        vec![
            a_coding_agent("claude", &["anthropic"]),
            a_coding_agent("cursor", &[]),
            a_coding_agent("opencode", &["openai_responses", "openai_chat"]),
            AgentInfo {
                id: "zebra-backend".to_string(),
                label: "zebra-backend".to_string(),
                kind: AgentKind::Model as i32,
                harness: String::new(),
                route_providers: Vec::new(),
            },
        ]
    );
}
