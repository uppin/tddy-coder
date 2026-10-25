//! Acceptance: a project-home session starts a session with any option the New-session form offers,
//! on its own host or another one, linked to itself as the orchestrator on whichever host holds
//! the new session (`#start-parity` 2/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-start-tool.md

mod support;

use std::path::Path;
use std::sync::Arc;

use support::{a_host, a_workspace_in, InProcessPeers, HOST_A, HOST_B, SILENT_HOST, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const START_SESSION: &str = "StartSession";

struct World {
    a: support::Host,
    b: support::Host,
    home: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    World { a, b, home }
}

impl World {
    async fn home_calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
            .into_inner()
    }

    async fn start_session_schema_offered_to_home(&self) -> serde_json::Value {
        let tools = self
            .a
            .daemon
            .list_exec_tools(Request::direct(ListExecToolsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                ..Default::default()
            }))
            .await
            .expect("the home session's tools are listed")
            .into_inner()
            .tools;
        let start = tools
            .iter()
            .find(|tool| tool.name == START_SESSION)
            .unwrap_or_else(|| {
                panic!(
                    "{START_SESSION} is offered to the home session; offered: {:?}",
                    tools.iter().map(|t| &t.name).collect::<Vec<_>>()
                )
            });
        serde_json::from_str(&start.input_schema_json).expect("the schema is JSON")
    }
}

fn started_session_id(response: &ExecuteToolResponse) -> String {
    assert!(!response.is_error, "{}", response.error_message);
    let started: serde_json::Value =
        serde_json::from_str(&response.result_json).expect("JSON result");
    started["session_id"]
        .as_str()
        .expect("the started session's id")
        .to_string()
}

fn changeset_of(sessions_base: &Path, session_id: &str) -> tddy_core::Changeset {
    tddy_core::read_changeset(&tddy_core::session_lifecycle::unified_session_dir_path(
        sessions_base,
        session_id,
    ))
    .expect("the started session has a changeset")
}

fn session_dir_exists(sessions_base: &Path, session_id: &str) -> bool {
    tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id).exists()
}

#[tokio::test]
async fn start_session_is_offered_with_every_eligible_host_and_the_daemons_recipes() {
    // Given
    let world = a_world().await;

    // When
    let schema = world.start_session_schema_offered_to_home().await;

    // Then
    assert_eq!(
        schema["properties"]["daemon_instance_id"]["enum"],
        serde_json::json!(["", HOST_A, HOST_B, SILENT_HOST])
    );
    let mut recipes = vec![String::new()];
    recipes.extend(
        tddy_workflow_recipes::approval_policy::supported_workflow_recipe_cli_names()
            .iter()
            .map(|r| r.to_string()),
    );
    assert_eq!(
        schema["properties"]["recipe"]["enum"],
        serde_json::json!(recipes)
    );
}

#[tokio::test]
async fn start_session_starts_on_the_branch_it_names_linked_to_the_orchestrator() {
    // Given
    let world = a_world().await;

    // When the home session starts a workspace on a branch it names
    let started = started_session_id(
        &world
            .home_calls(
                START_SESSION,
                serde_json::json!({
                    "session_type": "workspace",
                    "branch_worktree_intent": "new_branch_from_base",
                    "new_branch_name": "orchestrated/child",
                }),
            )
            .await,
    );

    // Then the session is on host-a, on that branch, linked to the home session
    let changeset = changeset_of(world.a.sessions.path(), &started);
    assert_eq!(changeset.branch.as_deref(), Some("orchestrated/child"));
    assert_eq!(
        changeset.orchestrator_session_id.as_deref(),
        Some(world.home.as_str())
    );
}

#[tokio::test]
async fn start_session_on_another_host_creates_the_session_there_linked_to_the_orchestrator() {
    // Given
    let world = a_world().await;

    // When the home session starts a workspace naming host-b
    let started = started_session_id(
        &world
            .home_calls(
                START_SESSION,
                serde_json::json!({
                    "session_type": "workspace",
                    "daemon_instance_id": HOST_B,
                    "branch_worktree_intent": "new_branch_from_base",
                    "new_branch_name": "orchestrated/on-b",
                }),
            )
            .await,
    );

    // Then it exists on host-b only, linked to the home session in host-b's own record
    assert!(session_dir_exists(world.b.sessions.path(), &started));
    assert!(!session_dir_exists(world.a.sessions.path(), &started));
    let changeset = changeset_of(world.b.sessions.path(), &started);
    assert_eq!(changeset.branch.as_deref(), Some("orchestrated/on-b"));
    assert_eq!(
        changeset.orchestrator_session_id.as_deref(),
        Some(world.home.as_str())
    );
}

#[tokio::test]
async fn a_branch_another_session_owns_is_reported_not_suffixed() {
    // Given a session already on the branch
    let world = a_world().await;
    let args = serde_json::json!({
        "session_type": "workspace",
        "branch_worktree_intent": "new_branch_from_base",
        "new_branch_name": "orchestrated/taken",
    });
    started_session_id(&world.home_calls(START_SESSION, args.clone()).await);

    // When the home session asks for the same branch again
    let second = world.home_calls(START_SESSION, args).await;

    // Then it is refused, naming the branch, rather than started on a suffixed one
    assert!(second.is_error);
    assert!(
        second.error_message.contains("orchestrated/taken"),
        "{}",
        second.error_message
    );
}

#[tokio::test]
async fn an_argument_the_form_does_not_offer_is_refused_by_name() {
    // Given
    let world = a_world().await;

    // When the home session asks for a role
    let refused = world
        .home_calls(
            START_SESSION,
            serde_json::json!({ "session_type": "workspace", "session_role": "project-home" }),
        )
        .await;

    // Then the tool refuses the argument by its name
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("session_role"),
        "{}",
        refused.error_message
    );
}
