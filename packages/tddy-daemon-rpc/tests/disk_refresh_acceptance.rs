//! Acceptance: `RefreshWorktreeDiskUsage` queues the Worktrees screen's size walk and returns at
//! once; the sizes and the progress arrive through `ListWorktrees`, as they do on the screen
//! (`#start-parity` 8/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-disk-refresh.md

mod support;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    second_on_a: String,
    on_b: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let second_on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    World {
        a,
        b,
        home,
        on_a,
        second_on_a,
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

    async fn result(&self, tool: &str, args: serde_json::Value) -> serde_json::Value {
        let response = self.home_calls(tool, args).await;
        assert!(!response.is_error, "{}", response.error_message);
        serde_json::from_str(&response.result_json).expect("JSON result")
    }

    /// `ListWorktrees` (on `host`, `""` for this one), polled until every worktree is sized.
    async fn listing_once_sized(&self, host: &str) -> serde_json::Value {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let listing = self
                .result(
                    "ListWorktrees",
                    serde_json::json!({ "daemon_instance_id": host }),
                )
                .await;
            let sizing = &listing["sizing"];
            if sizing["total"].as_u64() > Some(0) && sizing["done"] == sizing["total"] {
                return listing;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the sizes never arrived; last: {listing}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

fn canonical(path: &str) -> String {
    Path::new(path)
        .canonicalize()
        .unwrap()
        .display()
        .to_string()
}

fn row_for<'a>(listing: &'a serde_json::Value, worktree: &Path) -> &'a serde_json::Value {
    let wanted = worktree.canonicalize().unwrap().display().to_string();
    listing["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| canonical(row["path"].as_str().unwrap()) == wanted)
        .unwrap_or_else(|| panic!("{wanted} is listed: {listing}"))
}

#[tokio::test]
async fn a_refresh_returns_at_once_and_every_size_arrives_through_list_worktrees() {
    // Given
    let world = a_world().await;

    // When the home session refreshes every worktree of its project
    let queued = world
        .result("RefreshWorktreeDiskUsage", serde_json::json!({}))
        .await;

    // Then it says how many it queued, and the list ends with every one sized and cached
    assert!(queued["queued"].as_u64().unwrap() >= 2, "{queued}");
    let listing = world.listing_once_sized("").await;
    for row in listing["worktrees"].as_array().unwrap() {
        assert_eq!(row["size_status"], "cached", "{row}");
        assert!(row["disk_bytes"].as_u64().unwrap() > 0, "{row}");
    }
}

#[tokio::test]
async fn refreshing_one_worktree_recalculates_only_it() {
    // Given every worktree sized once
    let world = a_world().await;
    world
        .result("RefreshWorktreeDiskUsage", serde_json::json!({}))
        .await;
    let before = world.listing_once_sized("").await;
    let grown = world.a.worktree_of(&world.on_a);
    let untouched = world.a.worktree_of(&world.second_on_a);
    let untouched_calculated_at =
        row_for(&before, &untouched)["size_calculated_at_unix_ms"].clone();
    let grown_bytes_before = row_for(&before, &grown)["disk_bytes"].as_u64().unwrap();

    // When one worktree grows and only it is refreshed
    std::fs::write(grown.join("ballast.bin"), vec![0u8; 512 * 1024]).unwrap();
    let queued = world
        .result(
            "RefreshWorktreeDiskUsage",
            serde_json::json!({ "path": grown.display().to_string() }),
        )
        .await;
    assert_eq!(queued["queued"], 1);

    // Then it reads larger, and the other was not recalculated
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let after = loop {
        let listing = world.listing_once_sized("").await;
        if row_for(&listing, &grown)["disk_bytes"].as_u64().unwrap() > grown_bytes_before {
            break listing;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the new size never arrived"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(
        row_for(&after, &untouched)["size_calculated_at_unix_ms"],
        untouched_calculated_at
    );
}

#[tokio::test]
async fn an_unknown_worktree_is_refused_naming_it() {
    // Given
    let world = a_world().await;

    // When
    let refused = world
        .home_calls(
            "RefreshWorktreeDiskUsage",
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
async fn another_hosts_worktrees_are_refreshed_there() {
    // Given
    let world = a_world().await;

    // When the home session refreshes host-b's worktrees
    world
        .result(
            "RefreshWorktreeDiskUsage",
            serde_json::json!({ "daemon_instance_id": HOST_B }),
        )
        .await;

    // Then host-b's worktree is sized, read through host-b's list
    let listing = world.listing_once_sized(HOST_B).await;
    let row = row_for(&listing, &world.b.worktree_of(&world.on_b));
    assert_eq!(row["size_status"], "cached", "{row}");
}
