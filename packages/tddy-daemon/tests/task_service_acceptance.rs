//! Acceptance tests for the `tasks.TaskService` RPC service.
//!
//! Tests are driven through `RpcBridge` — no network, no HTTP transport.
//! Follows the pattern established in `vm_service_acceptance.rs`.

use async_trait::async_trait;
use bytes::Bytes;
use prost::Message;
use std::sync::Arc;
use tddy_rpc::{Code, RequestMetadata, ResponseBody, RpcBridge, RpcMessage};
use tddy_service::proto::tasks::{
    task_list_event, CancelTaskRequest, CancelTaskResponse, GetTaskRequest, GetTaskResponse,
    ListTasksRequest, ListTasksResponse, SendInputRequest, SendInputResponse, TaskListEvent,
    TaskOutputEvent, TaskServiceServer, TaskStatusProto, WatchTaskListRequest, WatchTaskRequest,
};
use tddy_task::{ChannelKind, TaskBody, TaskChannel, TaskContext, TaskRegistry, TaskStatus};

use tddy_daemon::task_service::{SessionUserResolver, TaskServiceImpl};

const GOOD_TOKEN: &str = "valid-token";
const BAD_TOKEN: &str = "bogus-token";

fn test_resolver() -> SessionUserResolver {
    Arc::new(|token: &str| {
        if token == GOOD_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

fn test_bridge(registry: TaskRegistry) -> RpcBridge<TaskServiceServer<TaskServiceImpl>> {
    let svc = TaskServiceImpl::new(registry, test_resolver());
    RpcBridge::new(TaskServiceServer::new(svc))
}

async fn call<Req: Message, Resp: Message + Default>(
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

async fn call_streaming<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<TaskServiceServer<TaskServiceImpl>>,
    method: &str,
    req: Req,
) -> Vec<Resp> {
    let payload = req.encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", method, &[msg])
        .await
        .expect("bridge dispatch must not fail at transport level");
    let mut rx = match result {
        ResponseBody::Streaming(rx) => rx,
        _ => panic!("expected Streaming for server-streaming method {method}"),
    };
    let mut messages = Vec::new();
    while let Some(chunk) = rx.recv().await {
        let bytes = chunk.expect("stream chunk must not be an error");
        messages.push(Resp::decode(&bytes[..]).expect("decode stream message"));
    }
    messages
}

/// Collect events from a persistent server-streaming RPC until no event arrives for `idle`.
///
/// Used for `WatchTaskList` which keeps the stream open indefinitely — a per-event idle timeout
/// lets the test collect the expected events without waiting for the stream to close naturally.
async fn collect_streaming_until_idle<Req: Message, Resp: Message + Default>(
    bridge: &RpcBridge<TaskServiceServer<TaskServiceImpl>>,
    method: &str,
    req: Req,
    idle: std::time::Duration,
) -> Vec<Resp> {
    let payload = req.encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", method, &[msg])
        .await
        .expect("bridge dispatch must not fail at transport level");
    let mut rx = match result {
        ResponseBody::Streaming(rx) => rx,
        _ => panic!("expected Streaming for server-streaming method {method}"),
    };
    let mut messages = Vec::new();
    loop {
        match tokio::time::timeout(idle, rx.recv()).await {
            Ok(Some(chunk)) => {
                let bytes = chunk.expect("stream chunk must not be an error");
                messages.push(Resp::decode(&bytes[..]).expect("decode stream message"));
            }
            Ok(None) => break, // stream closed
            Err(_) => break,   // idle timeout — no more events expected
        }
    }
    messages
}

async fn assert_unauthenticated(
    bridge: &RpcBridge<TaskServiceServer<TaskServiceImpl>>,
    method: &str,
    payload: Vec<u8>,
) {
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", method, &[msg])
        .await;
    match result {
        Err(status) => assert_eq!(
            status.code,
            Code::Unauthenticated,
            "expected Unauthenticated for method {method}, got {:?}",
            status.code
        ),
        Ok(_) => panic!("expected Unauthenticated error for method {method} with bad token"),
    }
}

// ─── Auth tests ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_tasks_with_bad_token_returns_unauthenticated() {
    // Given
    let bridge = test_bridge(TaskRegistry::new());
    // When / Then
    assert_unauthenticated(
        &bridge,
        "ListTasks",
        ListTasksRequest {
            session_token: BAD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        }
        .encode_to_vec(),
    )
    .await;
}

#[tokio::test]
async fn get_task_with_bad_token_returns_unauthenticated() {
    // Given
    let bridge = test_bridge(TaskRegistry::new());
    // When / Then
    assert_unauthenticated(
        &bridge,
        "GetTask",
        GetTaskRequest {
            session_token: BAD_TOKEN.to_string(),
            task_id: "any".to_string(),
            daemon_instance_id: String::new(),
        }
        .encode_to_vec(),
    )
    .await;
}

#[tokio::test]
async fn cancel_task_with_bad_token_returns_unauthenticated() {
    // Given
    let bridge = test_bridge(TaskRegistry::new());
    // When / Then
    assert_unauthenticated(
        &bridge,
        "CancelTask",
        CancelTaskRequest {
            session_token: BAD_TOKEN.to_string(),
            task_id: "any".to_string(),
            daemon_instance_id: String::new(),
        }
        .encode_to_vec(),
    )
    .await;
}

#[tokio::test]
async fn send_input_with_bad_token_returns_unauthenticated() {
    // Given
    let bridge = test_bridge(TaskRegistry::new());
    // When / Then
    assert_unauthenticated(
        &bridge,
        "SendInput",
        SendInputRequest {
            session_token: BAD_TOKEN.to_string(),
            task_id: "any".to_string(),
            channel_id: "0".to_string(),
            data: vec![],
            daemon_instance_id: String::new(),
        }
        .encode_to_vec(),
    )
    .await;
}

// ─── ListTasks / GetTask ───────────────────────────────────────────────────────

#[tokio::test]
async fn list_and_get_reflect_registered_task() {
    // Given — a task registered directly in the registry
    let registry = TaskRegistry::new();
    let ch = TaskChannel::output_only("0", "combined", ChannelKind::Combined);
    let handle = registry
        .spawn(
            ImmediateBody {
                result: TaskStatus::Completed { exit_code: Some(0) },
            },
            "execute_tool:Read",
            "test-session",
            vec![ch],
        )
        .await;
    let task_id = handle.id.0.clone();

    // Let it complete.
    wait_terminal(&handle).await;

    let bridge = test_bridge(registry);

    // When — ListTasks
    let list: ListTasksResponse = call(
        &bridge,
        "ListTasks",
        ListTasksRequest {
            session_token: GOOD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then — task appears
    assert!(
        list.tasks.iter().any(|t| t.task_id == task_id),
        "ListTasks must include the registered task"
    );

    // When — GetTask
    let get: GetTaskResponse = call(
        &bridge,
        "GetTask",
        GetTaskRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: task_id.clone(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then
    let info = get.task.expect("GetTask must return a TaskInfo");
    assert_eq!(info.task_id, task_id);
    assert_eq!(info.kind, "execute_tool:Read");
    // TaskStatusProto::TASK_STATUS_COMPLETED = 3
    assert_eq!(info.status, 3, "status must be COMPLETED");
}

// ─── WatchTask ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn watch_task_replays_then_streams_live() {
    // Given — a channel with pre-existing capture and a pending task
    let registry = TaskRegistry::new();
    let ch = TaskChannel::output_only("0", "combined", ChannelKind::Combined);

    // Write some bytes before the task is even running (simulates replay)
    ch.write(Bytes::from("pre:"));

    let body = WriteThenCompleteBody {
        channel_id: "0".to_string(),
        data: Bytes::from("live"),
    };
    let handle = registry
        .spawn(body, "shell", "test-session", vec![ch])
        .await;
    let task_id = handle.id.0.clone();

    let bridge = test_bridge(registry);

    // When — WatchTask
    let events: Vec<TaskOutputEvent> = call_streaming(
        &bridge,
        "WatchTask",
        WatchTaskRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: task_id.clone(),
            channel_id: "0".to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then — first event(s) are replayed, last event carries terminal status
    assert!(!events.is_empty(), "WatchTask must emit at least one event");

    let replay_events: Vec<_> = events.iter().filter(|e| e.is_replay).collect();
    assert!(
        !replay_events.is_empty(),
        "must emit at least one replay event for pre-existing capture"
    );

    let replay_data: Vec<u8> = replay_events
        .iter()
        .flat_map(|e| e.data.iter().copied())
        .collect();
    assert!(
        replay_data.starts_with(b"pre:"),
        "replay data must start with pre-existing bytes; got {:?}",
        String::from_utf8_lossy(&replay_data)
    );

    let last = events.last().unwrap();
    assert_ne!(
        last.status, 0,
        "final event must carry a non-zero (terminal) status"
    );
}

#[tokio::test]
async fn watch_task_with_remote_daemon_instance_id_returns_failed_precondition() {
    // Given
    let registry = TaskRegistry::new();
    let handle = registry
        .spawn(
            ImmediateBody {
                result: TaskStatus::Completed { exit_code: Some(0) },
            },
            "test",
            "session",
            vec![],
        )
        .await;
    let task_id = handle.id.0.clone();
    let bridge = test_bridge(registry);

    // When — request with a remote daemon_instance_id
    let payload = WatchTaskRequest {
        session_token: GOOD_TOKEN.to_string(),
        task_id,
        channel_id: String::new(),
        daemon_instance_id: "remote-daemon-123".to_string(),
    }
    .encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", "WatchTask", &[msg])
        .await;

    // Then — must be FailedPrecondition
    match result {
        Err(status) => assert_eq!(
            status.code,
            Code::FailedPrecondition,
            "remote WatchTask must return FailedPrecondition"
        ),
        Ok(_) => panic!("expected FailedPrecondition for remote daemon_instance_id"),
    }
}

// ─── CancelTask ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancel_task_flips_status_to_cancelled() {
    // Given — a task that only completes on cancel signal
    let registry = TaskRegistry::new();
    let handle = registry
        .spawn(WaitForCancelBody, "test", "session", vec![])
        .await;
    let task_id = handle.id.0.clone();
    let bridge = test_bridge(registry);

    // When
    let resp: CancelTaskResponse = call(
        &bridge,
        "CancelTask",
        CancelTaskRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: task_id.clone(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then — RPC ok
    assert!(resp.ok, "CancelTask must return ok=true");

    // And task eventually reaches Cancelled
    wait_terminal(&handle).await;
    assert_eq!(
        handle.status(),
        TaskStatus::Cancelled,
        "task must be Cancelled after CancelTask"
    );
}

#[tokio::test]
async fn cancel_nonexistent_task_returns_not_found() {
    // Given
    let bridge = test_bridge(TaskRegistry::new());
    // When
    let payload = CancelTaskRequest {
        session_token: GOOD_TOKEN.to_string(),
        task_id: "nonexistent-id".to_string(),
        daemon_instance_id: String::new(),
    }
    .encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", "CancelTask", &[msg])
        .await;
    // Then
    match result {
        Err(status) => assert_eq!(status.code, Code::NotFound),
        Ok(_) => panic!("expected NotFound for nonexistent task"),
    }
}

// ─── SendInput ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn send_input_to_input_channel_returns_ok() {
    // Given — a task with an input-capable channel that stays Running until cancelled
    let registry = TaskRegistry::new();
    let (ch, _stdin_rx) = TaskChannel::new("0", "stdin", ChannelKind::Combined);
    let handle = registry
        .spawn(
            WaitForCancelBody, // stays Running so the channel is open when SendInput fires
            "test",
            "session",
            vec![ch],
        )
        .await;
    let task_id = handle.id.0.clone();
    let bridge = test_bridge(registry);

    // When
    let resp: SendInputResponse = call(
        &bridge,
        "SendInput",
        SendInputRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: task_id.clone(),
            channel_id: "0".to_string(),
            data: b"hello".to_vec(),
            daemon_instance_id: String::new(),
        },
    )
    .await;

    // Then
    assert!(
        resp.ok,
        "SendInput to an input-capable channel must return ok=true"
    );
}

#[tokio::test]
async fn send_input_to_output_only_channel_returns_failed_precondition() {
    // Given — a task with an output-only channel (no stdin)
    let registry = TaskRegistry::new();
    let ch = TaskChannel::output_only("0", "combined", ChannelKind::Combined);
    let handle = registry
        .spawn(
            ImmediateBody {
                result: TaskStatus::Completed { exit_code: Some(0) },
            },
            "test",
            "session",
            vec![ch],
        )
        .await;
    let task_id = handle.id.0.clone();
    let bridge = test_bridge(registry);

    // When
    let payload = SendInputRequest {
        session_token: GOOD_TOKEN.to_string(),
        task_id,
        channel_id: "0".to_string(),
        data: b"hello".to_vec(),
        daemon_instance_id: String::new(),
    }
    .encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", "SendInput", &[msg])
        .await;

    // Then
    match result {
        Err(status) => assert_eq!(
            status.code,
            Code::FailedPrecondition,
            "SendInput to output-only channel must return FailedPrecondition"
        ),
        Ok(_) => panic!("expected FailedPrecondition for output-only channel"),
    }
}

// ─── WatchTaskList ────────────────────────────────────────────────────────────

#[tokio::test]
async fn watch_task_list_with_bad_token_returns_unauthenticated() {
    // Given
    let bridge = test_bridge(TaskRegistry::new());
    // When / Then
    assert_unauthenticated(
        &bridge,
        "WatchTaskList",
        WatchTaskListRequest {
            session_token: BAD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        }
        .encode_to_vec(),
    )
    .await;
}

#[tokio::test]
async fn watch_task_list_initial_snapshot_includes_existing_tasks() {
    // Given — a task already in the registry before the stream is opened
    let registry = TaskRegistry::new();
    let handle = registry
        .spawn(
            ImmediateBody {
                result: TaskStatus::Completed { exit_code: Some(0) },
            },
            "execute_tool:Read",
            "test-session",
            vec![],
        )
        .await;
    let task_id = handle.id.0.clone();
    wait_terminal(&handle).await;

    let bridge = test_bridge(registry);

    // When — open WatchTaskList stream; collect until idle (stream stays open persistently)
    let events: Vec<TaskListEvent> = collect_streaming_until_idle(
        &bridge,
        "WatchTaskList",
        WatchTaskListRequest {
            session_token: GOOD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
        std::time::Duration::from_millis(500),
    )
    .await;

    // Then — at least one snapshot event contains the existing task
    let snapshot_events: Vec<_> = events.iter().filter(|e| e.is_snapshot).collect();
    assert!(
        !snapshot_events.is_empty(),
        "WatchTaskList must emit at least one snapshot event"
    );

    let found = snapshot_events.iter().any(|e| {
        if let Some(task_list_event::Event::TaskAdded(info)) = &e.event {
            info.task_id == task_id
        } else {
            false
        }
    });
    assert!(
        found,
        "snapshot must include the pre-existing task; got {} snapshot events",
        snapshot_events.len()
    );
}

#[tokio::test]
async fn watch_task_list_streams_task_added_for_newly_spawned_task() {
    // Given — empty registry, stream opened first
    let registry = TaskRegistry::new();
    let bridge = test_bridge(registry.clone());

    // Spawn a task after opening the stream (in a concurrent task)
    let registry_for_spawn = registry.clone();
    let spawn_task = tokio::spawn(async move {
        // Small yield to let the stream handler subscribe before spawning
        tokio::task::yield_now().await;
        registry_for_spawn
            .spawn(
                ImmediateBody {
                    result: TaskStatus::Completed { exit_code: Some(0) },
                },
                "shell",
                "test-session",
                vec![],
            )
            .await
    });

    // When — stream events; collect until idle (stream stays open persistently)
    let events: Vec<TaskListEvent> = collect_streaming_until_idle(
        &bridge,
        "WatchTaskList",
        WatchTaskListRequest {
            session_token: GOOD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
        std::time::Duration::from_millis(500),
    )
    .await;

    let spawned = spawn_task.await.unwrap();
    let spawned_id = spawned.id.0.clone();

    // Then — a task_added event (live, not snapshot) arrives for the new task
    let added_live = events.iter().any(|e| {
        !e.is_snapshot
            && matches!(
                &e.event,
                Some(task_list_event::Event::TaskAdded(info)) if info.task_id == spawned_id
            )
    });
    assert!(
        added_live,
        "must receive a live task_added event for newly spawned task; events: {}",
        events.len()
    );
}

#[tokio::test]
async fn watch_task_list_streams_task_updated_on_status_transition() {
    // Given — a long-running task
    let registry = TaskRegistry::new();
    let handle = registry
        .spawn(WaitForCancelBody, "test", "test-session", vec![])
        .await;
    let task_id = handle.id.0.clone();
    let bridge = test_bridge(registry.clone());

    // Cancel the task concurrently so the stream sees the Updated event
    let registry_for_cancel = registry.clone();
    let handle_id = handle.id.clone();
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        registry_for_cancel.cancel_task(&handle_id).await;
    });

    // When — stream events until the task reaches terminal; collect until idle
    let events: Vec<TaskListEvent> = collect_streaming_until_idle(
        &bridge,
        "WatchTaskList",
        WatchTaskListRequest {
            session_token: GOOD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
        std::time::Duration::from_millis(500),
    )
    .await;

    // Then — an Updated event arrives carrying a terminal status for the task
    let non_terminal = [
        TaskStatusProto::TaskStatusUnknown as i32,
        TaskStatusProto::TaskStatusPending as i32,
        TaskStatusProto::TaskStatusRunning as i32,
    ];
    let updated_terminal = events.iter().any(|e| {
        if let Some(task_list_event::Event::TaskUpdated(info)) = &e.event {
            info.task_id == task_id && !non_terminal.contains(&info.status)
        } else {
            false
        }
    });
    assert!(
        updated_terminal,
        "must receive task_updated event with terminal status; events: {}",
        events.len()
    );
}

#[tokio::test]
async fn watch_task_list_with_remote_daemon_instance_id_returns_failed_precondition() {
    // Given
    let bridge = test_bridge(TaskRegistry::new());
    // When — request with a non-empty daemon_instance_id
    let payload = WatchTaskListRequest {
        session_token: GOOD_TOKEN.to_string(),
        daemon_instance_id: "remote-daemon-456".to_string(),
    }
    .encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("tasks.TaskService", "WatchTaskList", &[msg])
        .await;
    // Then
    match result {
        Err(status) => assert_eq!(
            status.code,
            Code::FailedPrecondition,
            "remote WatchTaskList must return FailedPrecondition"
        ),
        Ok(_) => panic!("expected FailedPrecondition for remote daemon_instance_id"),
    }
}

// ─── Helper bodies ────────────────────────────────────────────────────────────

struct ImmediateBody {
    result: TaskStatus,
}

#[async_trait]
impl TaskBody for ImmediateBody {
    async fn run(self: Box<Self>, _ctx: TaskContext) -> TaskStatus {
        self.result
    }
}

struct WaitForCancelBody;

#[async_trait]
impl TaskBody for WaitForCancelBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        ctx.cancel_token().cancelled().await;
        TaskStatus::Cancelled
    }
}

/// Body that writes to a named channel, then completes.
struct WriteThenCompleteBody {
    channel_id: String,
    data: Bytes,
}

#[async_trait]
impl TaskBody for WriteThenCompleteBody {
    async fn run(self: Box<Self>, ctx: TaskContext) -> TaskStatus {
        if let Some(ch) = ctx.channel(&self.channel_id) {
            ch.write(self.data.clone());
        }
        TaskStatus::Completed { exit_code: Some(0) }
    }
}

// ─── Helper: wait until terminal ─────────────────────────────────────────────

async fn wait_terminal(handle: &tddy_task::TaskHandle) {
    let mut rx = handle.status_watch();
    loop {
        if rx.borrow().is_terminal() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}
