//! Acceptance tests for the `actions.ActionService` RPC service.
//!
//! Tests are driven through `RpcBridge` — no network, no HTTP transport.
//! Follows the pattern established in `task_service_acceptance.rs`.

use prost::Message;
use std::sync::Arc;
use tddy_actions::ActionCatalog;
use tddy_rpc::{RequestMetadata, ResponseBody, RpcBridge, RpcMessage};
use tddy_service::proto::actions::{
    ActionServiceServer, GetActionRequest, GetActionResponse, ListActionKindsRequest,
    ListActionKindsResponse, StartActionRequest, StartActionResponse,
};
use tddy_service::proto::tasks::{ListTasksRequest, ListTasksResponse, TaskServiceServer};
use tddy_task::TaskRegistry;

use tddy_daemon::action_service::ActionServiceImpl;
use tddy_daemon::task_service::{SessionUserResolver, TaskServiceImpl};

const GOOD_TOKEN: &str = "valid-token";

fn test_resolver() -> SessionUserResolver {
    Arc::new(|token: &str| {
        if token == GOOD_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

struct ActionTaskHarness {
    action_bridge: RpcBridge<ActionServiceServer<ActionServiceImpl>>,
    task_bridge: RpcBridge<TaskServiceServer<TaskServiceImpl>>,
}

impl ActionTaskHarness {
    fn new() -> Self {
        let registry = TaskRegistry::new();
        let resolver = test_resolver();
        let action_bridge = RpcBridge::new(ActionServiceServer::new(ActionServiceImpl::new(
            registry.clone(),
            ActionCatalog::new(),
            resolver.clone(),
        )));
        let task_bridge = RpcBridge::new(TaskServiceServer::new(TaskServiceImpl::new(
            registry.clone(),
            resolver,
        )));
        Self {
            action_bridge,
            task_bridge,
        }
    }
}

async fn call_action<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<ActionServiceServer<ActionServiceImpl>>,
    method: &str,
    req: Req,
) -> Resp {
    let payload = req.encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("actions.ActionService", method, &[msg])
        .await
        .expect("bridge dispatch must not fail at transport level");
    let chunks = match result {
        ResponseBody::Complete(c) => c,
        _ => panic!("expected Complete for unary method {method}"),
    };
    assert_eq!(
        chunks.len(),
        1,
        "unary method {method} must return exactly 1 chunk"
    );
    Resp::decode(&chunks[0][..]).expect("decode response")
}

async fn call_tasks<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<TaskServiceServer<TaskServiceImpl>>,
    method: &str,
    req: Req,
) -> Resp {
    let payload = req.encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", method, &[msg])
        .await
        .expect("bridge dispatch must not fail at transport level");
    let chunks = match result {
        ResponseBody::Complete(c) => c,
        _ => panic!("expected Complete for unary method {method}"),
    };
    assert_eq!(
        chunks.len(),
        1,
        "unary method {method} must return exactly 1 chunk"
    );
    Resp::decode(&chunks[0][..]).expect("decode response")
}

#[tokio::test]
async fn start_action_bash_returns_task_id_visible_in_list_tasks() {
    // Given
    let harness = ActionTaskHarness::new();
    let params_json = serde_json::json!({
        "id": "bash-echo",
        "command": ["echo", "hello-actions"]
    })
    .to_string();

    // When — StartAction for bash
    let start: StartActionResponse = call_action(
        &harness.action_bridge,
        "StartAction",
        StartActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            session_id: "test-session".to_string(),
            kind: "bash".to_string(),
            params_json,
            sandbox: false,
        },
    )
    .await;

    // Then — RPC succeeds with a task id
    assert!(start.ok, "StartAction must return ok=true");
    assert!(
        !start.task_id.is_empty(),
        "StartAction must return a non-empty task_id"
    );

    // When — ListTasks on the shared registry
    let list: ListTasksResponse = call_tasks(
        &harness.task_bridge,
        "ListTasks",
        ListTasksRequest {
            session_token: GOOD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then — spawned bash task is visible
    let found = list
        .tasks
        .iter()
        .find(|t| t.task_id == start.task_id)
        .expect("ListTasks must include the bash action task");
    assert_eq!(found.kind, "bash");

    // When — GetAction for the same task
    let get: GetActionResponse = call_action(
        &harness.action_bridge,
        "GetAction",
        GetActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: start.task_id.clone(),
        },
    )
    .await;

    // Then
    let action = get.action.expect("GetAction must return action metadata");
    assert_eq!(action.task_id, start.task_id);
    assert_eq!(action.kind, "bash");
    assert_eq!(action.session_id, "test-session");
}

#[tokio::test]
async fn list_action_kinds_includes_bash_with_input_schema() {
    // Given
    let harness = ActionTaskHarness::new();

    // When
    let list: ListActionKindsResponse = call_action(
        &harness.action_bridge,
        "ListActionKinds",
        ListActionKindsRequest {
            session_token: GOOD_TOKEN.to_string(),
        },
    )
    .await;

    // Then
    let bash = list
        .kinds
        .iter()
        .find(|k| k.kind == "bash")
        .expect("catalog must include bash kind");
    assert!(bash.has_input_schema);
    assert!(!bash.summary.is_empty());
}
