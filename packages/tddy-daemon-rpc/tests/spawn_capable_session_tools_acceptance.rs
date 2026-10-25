//! Acceptance: a session that can spawn conversations (a grill-me session) is offered the start
//! tools a project-home session has — and starts only in its own project, linked to itself
//! (`#start-parity` 9/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-spawn-conversation.md

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const START_TOOLS: [&str; 4] = [
    "StartSession",
    "GetSessionStart",
    "CancelSessionStart",
    "DescribeSessionStartOptions",
];

struct World {
    a: Host,
    spawner: String,
    ordinary: String,
}

fn with_recipe(host: &Host, session_id: &str, recipe: &str) {
    let dir =
        tddy_core::session_lifecycle::unified_session_dir_path(host.sessions.path(), session_id);
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.recipe = Some(recipe.to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let spawner = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    with_recipe(&a, &spawner, "grill-me");
    let ordinary = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    with_recipe(&a, &ordinary, "tdd");
    World {
        a,
        spawner,
        ordinary,
    }
}

impl World {
    async fn offered_to(&self, session_id: &str) -> Vec<String> {
        self.a
            .daemon
            .list_exec_tools(Request::direct(ListExecToolsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                ..Default::default()
            }))
            .await
            .expect("the session's tools are listed")
            .into_inner()
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .collect()
    }

    async fn spawner_calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.spawner.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the spawning session: {s}"))
            .into_inner()
    }
}

fn result_of(response: &ExecuteToolResponse) -> serde_json::Value {
    assert!(!response.is_error, "{}", response.error_message);
    serde_json::from_str(&response.result_json).expect("JSON result")
}

#[tokio::test]
async fn a_session_that_spawns_conversations_is_offered_the_start_tools() {
    // Given
    let world = a_world().await;

    // When
    let offered = world.offered_to(&world.spawner).await;

    // Then
    for tool in START_TOOLS {
        assert!(
            offered.contains(&tool.to_string()),
            "{tool} is not offered: {offered:?}"
        );
    }
}

#[tokio::test]
async fn an_ordinary_session_is_not_offered_them() {
    // Given
    let world = a_world().await;

    // When
    let offered = world.offered_to(&world.ordinary).await;

    // Then
    for tool in START_TOOLS {
        assert!(
            !offered.contains(&tool.to_string()),
            "{tool} is offered: {offered:?}"
        );
    }
}

#[tokio::test]
async fn a_spawning_session_starts_in_its_own_project_linked_to_itself() {
    // Given
    let world = a_world().await;

    // When it starts a workspace
    let handle = result_of(
        &world
            .spawner_calls(
                "StartSession",
                serde_json::json!({
                    "session_type": "workspace",
                    "branch_worktree_intent": "new_branch_from_base",
                    "new_branch_name": "spawned/by-grill-me",
                }),
            )
            .await,
    );
    let start_id = handle["start_id"].as_str().expect("a start_id").to_string();

    // Then the start ends with a session of its own project, linked to it
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let done = loop {
        let record = result_of(
            &world
                .spawner_calls(
                    "GetSessionStart",
                    serde_json::json!({ "start_id": start_id }),
                )
                .await,
        );
        if record["state"] != "running" {
            break record;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the start never ended: {record}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(done["state"], "succeeded", "{done}");
    let child_dir = tddy_core::session_lifecycle::unified_session_dir_path(
        world.a.sessions.path(),
        done["session_id"].as_str().unwrap(),
    );
    let metadata = tddy_core::read_session_metadata(&child_dir).unwrap();
    assert_eq!(metadata.project_id, THIS_PROJECT);
    let changeset = tddy_core::read_changeset(&child_dir).unwrap();
    assert_eq!(
        changeset.orchestrator_session_id.as_deref(),
        Some(world.spawner.as_str())
    );
}
