//! Acceptance: `DescribeSessionStartOptions` answers a host's choices from the same RPCs the
//! New-session form fills its pickers from, on the home session's host or another
//! (`#start-parity` 5/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-start-options.md

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use support::{a_host, a_workspace_in, git, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::catalog::{
    CatalogService, ListAgentModelsRequest, ListAgentsRequest, ListSubagentsRequest,
};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::project::{ListProjectBranchesRequest, ProjectService};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const DESCRIBE: &str = "DescribeSessionStartOptions";

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    World { a, b, home, on_a }
}

impl World {
    async fn options(&self, query: serde_json::Value) -> serde_json::Value {
        let response: ExecuteToolResponse = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: DESCRIBE.to_string(),
                args_json: query.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{DESCRIBE} is served to the home session: {s}"))
            .into_inner();
        assert!(!response.is_error, "{}", response.error_message);
        serde_json::from_str(&response.result_json).expect("JSON result")
    }
}

fn project_repo_of(host: &Host) -> PathBuf {
    let project = tddy_projects::project_storage::find_project(
        &host.sessions.path().join("projects"),
        THIS_PROJECT,
    )
    .unwrap()
    .expect("the project is registered");
    PathBuf::from(project.main_repo_path)
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .expect("an array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_agents_offered_are_the_hosts_agent_catalog() {
    // Given what the form's agent picker is filled from
    let world = a_world().await;
    let catalog = world
        .a
        .daemon
        .list_agents(Request::direct(ListAgentsRequest {}))
        .await
        .unwrap()
        .into_inner()
        .agents;

    // When
    let options = world.options(serde_json::json!({})).await;

    // Then
    let offered: Vec<(String, String)> = options["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["id"].as_str().unwrap().to_string(),
                a["label"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let expected: Vec<(String, String)> = catalog.into_iter().map(|a| (a.id, a.label)).collect();
    assert_eq!(offered, expected);
}

#[tokio::test]
async fn the_subagents_offered_are_qualified_by_their_host() {
    // Given what the form's agent picker is filled from
    let world = a_world().await;
    let catalog = world
        .a
        .daemon
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .unwrap()
        .into_inner()
        .subagents;

    // When
    let options = world.options(serde_json::json!({})).await;

    // Then
    let expected: Vec<String> = catalog
        .into_iter()
        .map(|s| format!("{}@{}", s.name, HOST_A))
        .collect();
    assert_eq!(strings(&options["subagents"]), expected);
}

#[tokio::test]
async fn the_models_offered_are_the_catalogs_models_for_the_session_types_agent() {
    // Given what the form's model picker is filled from for a claude-cli session
    let world = a_world().await;
    let catalog = world
        .a
        .daemon
        .list_agent_models(Request::direct(ListAgentModelsRequest {
            session_token: TEST_TOKEN.to_string(),
            agent: "claude-cli".to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("the catalog lists claude-cli's models")
        .into_inner();

    // When
    let options = world
        .options(serde_json::json!({ "session_type": "claude-cli" }))
        .await;

    // Then
    assert_eq!(options["models"]["default"], catalog.default_model);
    let offered: Vec<String> = options["models"]["choices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect();
    let expected: Vec<String> = catalog.models.into_iter().map(|m| m.id).collect();
    assert_eq!(offered, expected);
}

#[tokio::test]
async fn the_sessions_that_can_seed_a_stack_on_this_host_are_offered() {
    // Given a session of the project on host-a, on a branch of its own and not in any stack
    let world = a_world().await;

    // When
    let options = world.options(serde_json::json!({})).await;

    // Then it is offered as a stack base
    let bases: Vec<String> = options["pr_stack_base_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["session_id"].as_str().unwrap().to_string())
        .collect();
    assert!(bases.contains(&world.on_a), "{bases:?}");
}

#[tokio::test]
async fn another_hosts_branches_are_that_hosts_project_checkout() {
    // Given a branch only host-b's checkout of the project has
    let world = a_world().await;
    let repo_b = project_repo_of(&world.b);
    git(&repo_b, &["branch", "only-on-b"]);
    git(&repo_b, &["push", "-q", "origin", "only-on-b"]);
    let b_branches = world
        .b
        .daemon
        .list_project_branches(Request::direct(ListProjectBranchesRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: THIS_PROJECT.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .expect("host-b lists its branches")
        .into_inner()
        .branches;

    // When the home session describes host-b, and its own host
    let on_b = world
        .options(serde_json::json!({ "daemon_instance_id": HOST_B }))
        .await;
    let on_a = world.options(serde_json::json!({})).await;

    // Then host-b's answer is host-b's list, and host-a's does not have the branch
    assert_eq!(strings(&on_b["branches"]), b_branches);
    assert!(strings(&on_b["branches"])
        .iter()
        .any(|b| b.ends_with("only-on-b")));
    assert!(!strings(&on_a["branches"])
        .iter()
        .any(|b| b.ends_with("only-on-b")));
    assert_eq!(on_b["daemon_instance_id"], HOST_B);
}
