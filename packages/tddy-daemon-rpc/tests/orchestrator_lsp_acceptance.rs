//! Acceptance: an orchestrator's LSP tools are admitted with a `root` in its reach; it adds roots —
//! at once inside its reach, through the operator's answer outside it — lists them, and removes the
//! ones it added without asking, never one it did not add (`#orchestra` 8/9).
//!
//! Feature doc: docs/ft/coder/reusable-lsp.md.

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::activity::{
    ActivityService as _, AnswerPathAccessRequest, ListPathAccessRequestsRequest,
};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const WAIT: Duration = Duration::from_secs(10);

struct World {
    a: Arc<Host>,
    orchestrator: String,
    sibling: String,
    outside: tempfile::TempDir,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    let sibling = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    World {
        a: Arc::new(a),
        orchestrator,
        sibling,
        outside: tempfile::tempdir().unwrap(),
    }
}

async fn call(
    host: &Host,
    session_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> ExecuteToolResponse {
    host.daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
            tool_name: tool.to_string(),
            args_json: args.to_string(),
            daemon_instance_id: String::new(),
            conversation_id: String::new(),
        }))
        .await
        .map(|r| r.into_inner())
        .unwrap_or_else(|status| ExecuteToolResponse {
            is_error: true,
            error_message: status.message().to_string(),
            ..Default::default()
        })
}

impl World {
    async fn calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        call(&self.a, &self.orchestrator, tool, args).await
    }

    fn sibling_worktree(&self) -> PathBuf {
        self.a.worktree_of(&self.sibling).canonicalize().unwrap()
    }

    async fn listed_roots(&self) -> Vec<String> {
        let listed = self.calls("ListLspRoots", serde_json::json!({})).await;
        assert!(!listed.is_error, "{}", listed.error_message);
        let answer: serde_json::Value = serde_json::from_str(&listed.result_json).unwrap();
        answer["roots"]
            .as_array()
            .expect("a roots array")
            .iter()
            .map(|r| r.as_str().unwrap_or_default().to_string())
            .collect()
    }

    async fn pending_access_requests(&self) -> Vec<String> {
        self.a
            .daemon
            .connection()
            .activity_service()
            .list_path_access_requests(Request::direct(ListPathAccessRequestsRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.orchestrator.clone(),
            }))
            .await
            .map(|r| {
                r.into_inner()
                    .requests
                    .into_iter()
                    .map(|r| r.request_id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[tokio::test]
async fn an_orchestrators_lsp_tool_naming_a_sibling_root_is_not_refused_as_read_only() {
    // Given an orchestrator and a sibling session's worktree
    let world = a_world().await;

    // When it asks for symbols in the sibling's worktree
    let symbols = world
        .calls(
            "LspSymbols",
            serde_json::json!({ "root": world.sibling_worktree(), "target": "src/lib.rs" }),
        )
        .await;

    // Then the call is admitted for its role (whatever the language server then answers)
    assert!(
        !symbols.error_message.contains("read-only"),
        "{}",
        symbols.error_message
    );
}

#[tokio::test]
async fn adding_a_root_inside_reach_lists_it_without_a_request() {
    // Given an orchestrator
    let world = a_world().await;

    // When it adds the sibling's worktree as a root
    let added = world
        .calls(
            "AddLspRoot",
            serde_json::json!({ "root": world.sibling_worktree(), "reason": "trace the API" }),
        )
        .await;

    // Then it is listed, and the operator was asked nothing
    assert!(!added.is_error, "{}", added.error_message);
    assert_eq!(
        world.listed_roots().await,
        vec![world.sibling_worktree().display().to_string()]
    );
    assert_eq!(world.pending_access_requests().await, Vec::<String>::new());
}

#[tokio::test]
async fn adding_a_root_outside_reach_asks_the_operator_and_allow_adds_it() {
    // Given an orchestrator and a directory outside its reach
    let world = a_world().await;
    let outside = world.outside.path().canonicalize().unwrap();

    // When it adds that directory as a root, and the operator allows the request it raised
    let (a, orchestrator, root) = (
        Arc::clone(&world.a),
        world.orchestrator.clone(),
        outside.clone(),
    );
    let adding = tokio::spawn(async move {
        call(
            &a,
            &orchestrator,
            "AddLspRoot",
            serde_json::json!({ "root": root, "reason": "index the vendored SDK" }),
        )
        .await
    });
    let deadline = tokio::time::Instant::now() + WAIT;
    let request_id = loop {
        if let Some(id) = world.pending_access_requests().await.into_iter().next() {
            break id;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no access request was raised"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    world
        .a
        .daemon
        .connection()
        .activity_service()
        .answer_path_access(Request::direct(AnswerPathAccessRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: world.orchestrator.clone(),
            request_id,
            allow: true,
        }))
        .await
        .expect("the answer is taken");

    // Then the root is added
    let added = tokio::time::timeout(WAIT, adding)
        .await
        .expect("answered")
        .expect("task");
    assert!(!added.is_error, "{}", added.error_message);
    assert_eq!(
        world.listed_roots().await,
        vec![outside.display().to_string()]
    );
}

#[tokio::test]
async fn removing_a_root_this_session_added_drops_it_without_asking() {
    // Given an orchestrator that added a root
    let world = a_world().await;
    world
        .calls(
            "AddLspRoot",
            serde_json::json!({ "root": world.sibling_worktree(), "reason": "trace the API" }),
        )
        .await;

    // When it removes it
    let removed = world
        .calls(
            "RemoveLspRoot",
            serde_json::json!({ "root": world.sibling_worktree() }),
        )
        .await;

    // Then it is gone, and nothing was asked
    assert!(!removed.is_error, "{}", removed.error_message);
    assert_eq!(world.listed_roots().await, Vec::<String>::new());
    assert_eq!(world.pending_access_requests().await, Vec::<String>::new());
}

#[tokio::test]
async fn removing_a_root_this_session_did_not_add_is_refused() {
    // Given an orchestrator that added nothing
    let world = a_world().await;

    // When it removes the sibling's worktree
    let refused = world
        .calls(
            "RemoveLspRoot",
            serde_json::json!({ "root": world.sibling_worktree() }),
        )
        .await;

    // Then it is refused for not having added it
    assert!(refused.is_error, "{}", refused.result_json);
    assert!(
        refused.error_message.contains("AddLspRoot"),
        "{}",
        refused.error_message
    );
}
