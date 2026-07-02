//! Acceptance tests: `allowed_agents` config, `ListAgents` RPC, `ListTools` regression, and
//! `StartSession` rejection for agents outside the allowlist (PRD Testing Plan).

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::{Code, Request};
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListAgentsRequest, ListToolsRequest,
    StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Keeps the temp dir alive for the lifetime of the returned guard.
fn write_config(yaml: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

fn service_with_config(config: DaemonConfig, sessions_base: PathBuf) -> ConnectionServiceImpl {
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

/// **daemon_config_allowed_agents_deserializes**: YAML `allowed_agents` yields expected ids/labels;
/// unknown fields under an agent entry are rejected (`deny_unknown_fields`).
#[test]
fn daemon_config_allowed_agents_deserializes() {
    // Given
    let yaml = r#"
users:
  - github_user: "gh1"
    os_user: "os1"
allowed_tools:
  - path: /bin/true
    label: "true"
allowed_agents:
  - id: custom-a
    label: Custom A
  - id: custom-b
"#;
    let (_dir, path) = write_config(yaml);

    // When
    let config = DaemonConfig::load(&path).expect("config with allowed_agents must parse");

    // Then
    assert_eq!(config.allowed_agents.len(), 2);
    assert_eq!(config.allowed_agents[0].id, "custom-a");
    assert_eq!(config.allowed_agents[0].label.as_deref(), Some("Custom A"));
    assert_eq!(config.allowed_agents[1].id, "custom-b");
    assert_eq!(config.allowed_agents[1].label.as_deref(), None);

    let bad_yaml = r#"
users:
  - github_user: "gh1"
    os_user: "os1"
allowed_agents:
  - id: x
    typo_not_allowed: "y"
"#;
    let (_bad_dir, bad_path) = write_config(bad_yaml);
    assert!(
        DaemonConfig::load(&bad_path).is_err(),
        "unknown fields on allowed_agents entries must be rejected"
    );
}

/// **connection_service_list_agents_returns_config**: `ListAgents` matches config order and content;
/// hardcoded defaults such as `claude` must not appear when absent from config.
#[tokio::test]
async fn connection_service_list_agents_returns_config() {
    // Given
    let yaml = r#"
users:
  - github_user: "u"
    os_user: "u"
allowed_agents:
  - id: zebra-backend
    label: Zebra
  - id: alpha-backend
allowed_tools:
  - path: /bin/true
    label: t
"#;
    let (_dir, path) = write_config(yaml);
    let config = DaemonConfig::load(&path).unwrap();
    let _sessions_tmp = tempfile::tempdir().unwrap();
    let sessions_base = _sessions_tmp.path().to_path_buf();
    let service = service_with_config(config, sessions_base);

    // When
    let response = service
        .list_agents(Request::new(ListAgentsRequest {}))
        .await
        .expect("ListAgents must succeed");
    let agents = response.into_inner().agents;

    // Then
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].id, "zebra-backend");
    assert_eq!(agents[0].label, "Zebra");
    assert_eq!(agents[1].id, "alpha-backend");
    assert_eq!(agents[1].label, "alpha-backend");
    assert!(
        !agents.iter().any(|a| a.id == "claude"),
        "ListAgents must not inject hardcoded agent ids"
    );
}

/// **list_tools_unchanged_with_new_config_field**: `allowed_agents` alongside `allowed_tools` does
/// not change `ListTools` mapping.
#[tokio::test]
async fn list_tools_unchanged_with_new_config_field() {
    // Given
    let yaml = r#"
users:
  - github_user: "u"
    os_user: "u"
allowed_tools:
  - path: /first/tool
    label: First
  - path: /second/tool
allowed_agents:
  - id: only-agent
    label: Only
"#;
    let (_dir, path) = write_config(yaml);
    let config = DaemonConfig::load(&path).unwrap();
    let _sessions_tmp = tempfile::tempdir().unwrap();
    let sessions_base = _sessions_tmp.path().to_path_buf();
    let service = service_with_config(config, sessions_base);

    // When
    let response = service
        .list_tools(Request::new(ListToolsRequest {}))
        .await
        .expect("ListTools must succeed");
    let tools = response.into_inner().tools;

    // Then
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].path, "/first/tool");
    assert_eq!(tools[0].label, "First");
    assert_eq!(tools[1].path, "/second/tool");
    assert_eq!(tools[1].label, "/second/tool");
}

/// **start_session_unknown_agent_rejected**: non-empty `agent` not present in `allowed_agents` must
/// fail before LiveKit/project resolution with an actionable `invalid_argument` status.
#[tokio::test]
async fn start_session_unknown_agent_rejected() {
    // Given
    let yaml = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
allowed_agents:
  - id: permitted-agent
    label: Permitted
"#;
    let (_dir, path) = write_config(yaml);
    let config = DaemonConfig::load(&path).unwrap();
    let _sessions_tmp = tempfile::tempdir().unwrap();
    let sessions_base = _sessions_tmp.path().to_path_buf();
    let service = service_with_config(config, sessions_base);
    let request = Request::new(StartSessionRequest {
        session_token: "valid-token".to_string(),
        tool_path: "/bin/true".to_string(),
        project_id: "ignored-before-validation-order".to_string(),
        agent: "unknown-agent-id".to_string(),
        daemon_instance_id: String::new(),
        recipe: String::new(),
        session_type: String::new(),
        model: String::new(),
        branch_worktree_intent: String::new(),
        new_branch_name: String::new(),
        selected_integration_base_ref: String::new(),
        selected_branch_to_work_on: String::new(),
        initial_prompt: String::new(),
        permission_mode: String::new(),
        stack_parent: String::new(),
        sandbox: false,
        discovery_subagent: String::new(),
        fastcontext_url: String::new(),
        fastcontext_model: String::new(),
        fastcontext_max_turns: 0,
        subagent_replaces: String::new(),
    });

    // When
    let err = service
        .start_session(request)
        .await
        .expect_err("unknown agent must be rejected");

    // Then
    assert_eq!(err.code, Code::InvalidArgument);
    let msg = err.message.to_ascii_lowercase();
    assert!(
        msg.contains("allowed_agents") || msg.contains("not listed"),
        "message should mention allowlist; got: {}",
        err.message
    );
}
