//! Acceptance: a project-home session lists its project's worktrees and reads one, as the
//! Worktrees screen shows them, on its own host or another (`#start-parity` 3/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-worktree-list.md

mod support;

use std::sync::Arc;

use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    on_b: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    World {
        a,
        b,
        home,
        on_a,
        on_b,
    }
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
}

fn result_of(response: &ExecuteToolResponse) -> serde_json::Value {
    assert!(!response.is_error, "{}", response.error_message);
    serde_json::from_str(&response.result_json).expect("JSON result")
}

fn listed_paths(listing: &serde_json::Value) -> Vec<String> {
    listing["worktrees"]
        .as_array()
        .expect("a worktrees array")
        .iter()
        .map(|w| w["path"].as_str().unwrap().to_string())
        .collect()
}

fn canonical(path: &std::path::Path) -> String {
    path.canonicalize().unwrap().display().to_string()
}

fn canonical_paths(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .map(|p| canonical(std::path::Path::new(&p)))
        .collect()
}

#[tokio::test]
async fn list_worktrees_lists_the_projects_worktrees_on_this_host_with_their_branch() {
    // Given
    let world = a_world().await;

    // When
    let listing = result_of(
        &world
            .home_calls("ListWorktrees", serde_json::json!({}))
            .await,
    );

    // Then the project's session worktree on host-a is listed, with its branch, and counted
    let on_a = canonical(&world.a.worktree_of(&world.on_a));
    assert!(
        canonical_paths(listed_paths(&listing)).contains(&on_a),
        "{listing}"
    );
    assert_eq!(
        listing["total"].as_u64().unwrap() as usize,
        listed_paths(&listing).len()
    );
    let row = listing["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| canonical(std::path::Path::new(w["path"].as_str().unwrap())) == on_a)
        .unwrap();
    assert!(!row["branch_label"].as_str().unwrap().is_empty(), "{row}");
}

#[tokio::test]
async fn get_worktree_returns_the_row_the_list_holds_for_it() {
    // Given the row the list holds for the session's worktree
    let world = a_world().await;
    let listing = result_of(
        &world
            .home_calls("ListWorktrees", serde_json::json!({}))
            .await,
    );
    let on_a = canonical(&world.a.worktree_of(&world.on_a));
    let listed_row = listing["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| canonical(std::path::Path::new(w["path"].as_str().unwrap())) == on_a)
        .cloned()
        .expect("the worktree is listed");

    // When it is read on its own
    let detail = result_of(
        &world
            .home_calls(
                "GetWorktree",
                serde_json::json!({ "path": listed_row["path"] }),
            )
            .await,
    );

    // Then it is the same row
    assert_eq!(detail["worktree"], listed_row);
}

#[tokio::test]
async fn an_unknown_worktree_is_refused_naming_it() {
    // Given
    let world = a_world().await;

    // When
    let refused = world
        .home_calls(
            "GetWorktree",
            serde_json::json!({ "path": "/no/such/worktree" }),
        )
        .await;

    // Then
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("/no/such/worktree"),
        "{}",
        refused.error_message
    );
}

#[tokio::test]
async fn list_worktrees_lists_another_hosts_worktrees() {
    // Given a session of the project on host-b
    let world = a_world().await;

    // When the home session lists host-b's worktrees
    let listing = result_of(
        &world
            .home_calls(
                "ListWorktrees",
                serde_json::json!({ "daemon_instance_id": HOST_B }),
            )
            .await,
    );

    // Then host-b's worktree is listed, and host-a's is not
    let paths = canonical_paths(listed_paths(&listing));
    assert!(paths.contains(&canonical(&world.b.worktree_of(&world.on_b))));
    assert!(!paths.contains(&canonical(&world.a.worktree_of(&world.on_a))));
}
