//! Acceptance: `CatalogService.ListAgents` reports each agent's kind, harness and route support,
//! read from the coding-agent registry (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`.
//! Closes the tool-session provider picker's missing harness metadata (#801): a
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
    std::fs::write(&config_path, format!("allowed_agents:\n{allowed}")).unwrap();
    std::fs::write(
        home.path().join("users.yaml"),
        "- github_user: u\n  os_user: u\n",
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
        Arc::new(tddy_session_lifecycle::claude_cli_session::CliSessionManager::new()),
        tddy_session_lifecycle::test_util::builtin_coding_agents(),
    ));
    (home, daemon)
}

/// The row a registry agent is listed as: its spec's metadata, as `ListAgents` serves it.
fn a_coding_agent(id: &str, route_providers: &[&str]) -> AgentInfo {
    let registry = tddy_coding_agent_specs::CodingAgentRegistry::builtin_cached();
    let spec = registry.get(id).expect("the agent's spec loads");
    AgentInfo {
        id: id.to_string(),
        label: id.to_string(),
        kind: AgentKind::CodingAgent as i32,
        harness: id.to_string(),
        route_providers: route_providers.iter().map(|p| p.to_string()).collect(),
        session_type: format!("{id}-cli"),
        capabilities: spec
            .agent
            .capabilities
            .iter()
            .map(|capability| capability.name().to_string())
            .collect(),
        run_params: spec.argv_params().into_iter().collect(),
        permission_modes: spec.agent.permission_modes.clone(),
        default_model: spec.agent.models.default.clone(),
        placements: spec
            .agent
            .placements
            .iter()
            .map(|placement| placement.to_string())
            .collect(),
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
                ..Default::default()
            },
        ]
    );
}
