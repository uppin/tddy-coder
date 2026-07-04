//! Acceptance tests for sandboxed actions via `actions.ActionService`.
//!
//! Sandboxed actions use platform `spawn_plan` with egress-log or runner PTY bridging.
//!
//! Several imports and helpers here are consumed only by platform-gated tests
//! (`#[cfg(target_os = "macos")]` / `#[cfg(target_os = "linux")]`), so they read as unused when
//! building for other targets. Relax those lints file-wide rather than scatter per-item
//! `cfg_attr`s across a platform-multiplexed test file.
#![allow(dead_code, unused_imports)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use prost::Message;
use tddy_actions::build_action_fields_to_spec;
use tddy_actions::{ActionCatalog, BuildActionFields};
use tddy_rpc::{RequestMetadata, ResponseBody, RpcBridge, RpcMessage};
use tddy_service::proto::actions::{ActionServiceServer, StartActionRequest, StartActionResponse};
use tddy_service::proto::tasks::{
    GetTaskRequest, GetTaskResponse, ListTasksRequest, ListTasksResponse, TaskOutputEvent,
    TaskServiceServer, TaskStatusProto, WatchTaskRequest,
};
use tddy_task::TaskRegistry;

use tddy_daemon::action_service::ActionServiceImpl;
use tddy_daemon::sandbox_runtime::{attach_sandbox_request, SandboxRuntime};
use tddy_daemon::task_service::{SessionUserResolver, TaskServiceImpl};

const GOOD_TOKEN: &str = "valid-token";

/// Linux cgroups sandbox requires unprivileged user namespaces; macOS sandboxing does not.
/// Mirrors the self-skip contract in `tddy-sandbox-cgroups/tests/jail_smoke.rs`.
#[cfg(target_os = "linux")]
fn sandbox_userns_available() -> bool {
    tddy_sandbox_cgroups::unprivileged_userns_available()
}

#[cfg(not(target_os = "linux"))]
fn sandbox_userns_available() -> bool {
    true
}

fn test_resolver() -> SessionUserResolver {
    Arc::new(|token: &str| {
        if token == GOOD_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

struct SandboxHarness {
    action_bridge: RpcBridge<ActionServiceServer<ActionServiceImpl>>,
    task_bridge: RpcBridge<TaskServiceServer<TaskServiceImpl>>,
}

impl SandboxHarness {
    fn new() -> Self {
        let registry = TaskRegistry::new();
        let resolver = test_resolver();
        let action_bridge = RpcBridge::new(ActionServiceServer::new(ActionServiceImpl::new(
            registry.clone(),
            ActionCatalog::new(),
            resolver.clone(),
        )));
        let task_bridge = RpcBridge::new(TaskServiceServer::new(TaskServiceImpl::new(
            registry, resolver,
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
) -> Result<Resp, tddy_rpc::Status> {
    let payload = req.encode_to_vec();
    let msg = RpcMessage {
        payload,
        metadata: RequestMetadata::default(),
    };
    let result = bridge
        .handle_messages("actions.ActionService", method, &[msg])
        .await?;
    let chunks = match result {
        ResponseBody::Complete(c) => c,
        _ => panic!("expected Complete for unary method {method}"),
    };
    Ok(Resp::decode(&chunks[0][..]).expect("decode response"))
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
        .expect("bridge dispatch");
    let chunks = match result {
        ResponseBody::Complete(c) => c,
        _ => panic!("expected Complete for unary method {method}"),
    };
    Resp::decode(&chunks[0][..]).expect("decode response")
}

/// Await the task's actual completion event via `WatchTask`'s terminal-status frame — the
/// stream only closes once the registry reports a terminal status, so this has no fixed
/// timeout or polling interval to outrun under load (see `TaskServiceImpl::watch_task`).
async fn wait_for_terminal_task(
    bridge: &RpcBridge<TaskServiceServer<TaskServiceImpl>>,
    task_id: &str,
) -> GetTaskResponse {
    let events: Vec<TaskOutputEvent> = call_streaming(
        bridge,
        "WatchTask",
        WatchTaskRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: task_id.to_string(),
            channel_id: String::new(),
            daemon_instance_id: String::new(),
        },
    )
    .await;
    assert!(
        events
            .last()
            .is_some_and(|e| e.status != TaskStatusProto::TaskStatusUnknown as i32),
        "WatchTask stream must end with a terminal-status event; got {events:?}"
    );
    call_tasks(
        bridge,
        "GetTask",
        GetTaskRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: task_id.to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await
}

fn tddy_coder_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-coder")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-coder")
        })
}

async fn watch_task_replay_bytes(
    bridge: &RpcBridge<TaskServiceServer<TaskServiceImpl>>,
    task_id: &str,
) -> Vec<u8> {
    let events: Vec<TaskOutputEvent> = call_streaming(
        bridge,
        "WatchTask",
        WatchTaskRequest {
            session_token: GOOD_TOKEN.to_string(),
            task_id: task_id.to_string(),
            channel_id: String::new(),
            daemon_instance_id: String::new(),
        },
    )
    .await;
    events
        .into_iter()
        .filter(|e| e.is_replay)
        .flat_map(|e| e.data)
        .collect()
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
        .expect("bridge dispatch");
    let mut rx = match result {
        ResponseBody::Streaming(rx) => rx,
        _ => panic!("expected Streaming for {method}"),
    };
    let mut messages = Vec::new();
    while let Some(chunk) = rx.recv().await {
        let bytes = chunk.expect("stream chunk");
        messages.push(Resp::decode(&bytes[..]).expect("decode stream message"));
    }
    messages
}

#[tokio::test]
async fn sandboxed_bash_action_writes_to_output_dir() {
    if !sandbox_userns_available() {
        eprintln!(
            "SKIP: host forbids unprivileged user namespaces (cannot create the sandbox here)"
        );
        return;
    }

    // Given
    let harness = SandboxHarness::new();
    let output_dir = tempfile::tempdir().expect("output dir");
    let output_path = std::fs::canonicalize(output_dir.path()).expect("canonical output_dir");
    // cwd inside the jail is project_root (output_dir/sandbox/<action-id>)
    let marker = output_path
        .join("sandbox")
        .join("sandbox-bash")
        .join("sandbox-marker.txt");
    let params_json = serde_json::json!({
        "id": "sandbox-bash",
        "command": ["/bin/sh", "-c", "printf sandbox_ok > sandbox-marker.txt"],
        "output_dir": output_path.to_string_lossy(),
        "sandbox_recipe": "bash"
    })
    .to_string();

    // When — StartAction with sandbox=true
    let start: StartActionResponse = call_action(
        &harness.action_bridge,
        "StartAction",
        StartActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            session_id: "sandbox-session".to_string(),
            kind: "bash".to_string(),
            params_json,
            sandbox: true,
        },
    )
    .await
    .expect("StartAction must succeed");

    // Then — task registered and marker written under output_dir
    assert!(start.ok);
    assert!(!start.task_id.is_empty());

    let list: ListTasksResponse = call_tasks(
        &harness.task_bridge,
        "ListTasks",
        ListTasksRequest {
            session_token: GOOD_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .await;
    assert!(
        list.tasks.iter().any(|t| t.task_id == start.task_id),
        "sandboxed bash task must appear in ListTasks"
    );

    let terminal = wait_for_terminal_task(&harness.task_bridge, &start.task_id).await;
    let task = terminal.task.expect("task must be present");
    assert_eq!(
        task.status,
        TaskStatusProto::TaskStatusCompleted as i32,
        "sandboxed bash task must complete; exit_code={} error={}",
        task.exit_code,
        task.error_message
    );
    assert_eq!(
        task.exit_code, 0,
        "sandboxed bash must exit 0; stderr in result_json: {}",
        task.result_json
    );

    let written = std::fs::read_to_string(&marker).expect("marker file in project_root");
    assert_eq!(written, "sandbox_ok");
}

#[tokio::test]
async fn sandboxed_action_without_output_dir_is_rejected() {
    // Given
    let harness = SandboxHarness::new();
    let params_json = serde_json::json!({
        "id": "sandbox-missing-out",
        "command": ["echo", "hi"]
    })
    .to_string();

    // When / Then
    let err = call_action::<StartActionRequest, StartActionResponse>(
        &harness.action_bridge,
        "StartAction",
        StartActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            session_id: "sandbox-session".to_string(),
            kind: "bash".to_string(),
            params_json,
            sandbox: true,
        },
    )
    .await
    .expect_err("sandbox without output_dir must fail");

    assert!(
        err.message.contains("output_dir"),
        "error must mention output_dir; got {}",
        err.message
    );
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn sandboxed_tddy_coder_writes_under_output_dir() {
    // Given
    let harness = SandboxHarness::new();
    let output_dir = tempfile::tempdir().expect("output dir");
    let output_path = std::fs::canonicalize(output_dir.path()).expect("canonical output_dir");
    let coder = tddy_coder_binary();
    assert!(coder.is_file(), "build tddy-coder before running this test");
    let params_json = serde_json::json!({
        "id": "sandbox-tddy-coder",
        "goal": "plan",
        "agent": "stub",
        "recipe": "tdd",
        "coder_binary": coder.to_string_lossy(),
        "feature": "SKIP_QUESTIONS Build auth feature",
        "output_dir": output_path.to_string_lossy(),
        "sandbox_recipe": "generic",
        "stdin": "a\n"
    })
    .to_string();

    // When
    let start: StartActionResponse = call_action(
        &harness.action_bridge,
        "StartAction",
        StartActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            session_id: "sandbox-session".to_string(),
            kind: "tddy-coder".to_string(),
            params_json,
            sandbox: true,
        },
    )
    .await
    .expect("StartAction must succeed");

    // Then
    let terminal = wait_for_terminal_task(&harness.task_bridge, &start.task_id).await;
    let task = terminal.task.expect("task");
    assert_eq!(
        task.exit_code, 0,
        "tddy-coder must exit 0: {}",
        task.result_json
    );
    let sessions_dir = output_path.join("sessions");
    assert!(
        sessions_dir.is_dir(),
        "sessions dir must exist under output_dir at {}",
        sessions_dir.display()
    );
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn sandboxed_build_action_with_ro_mount() {
    // Given
    let registry = TaskRegistry::new();
    let output_dir = tempfile::tempdir().expect("output dir");
    let output_path = std::fs::canonicalize(output_dir.path()).expect("canonical output_dir");
    let input_name = "input.txt";
    std::fs::write(output_path.join(input_name), "seed").expect("write input");

    let mut spec = build_action_fields_to_spec(
        &output_path,
        BuildActionFields {
            id: "build-action".into(),
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                "printf built > out.txt".into(),
            ],
            env: BTreeMap::new(),
            input_globs: vec![(".".into(), vec![input_name.into()])],
            outputs: vec![],
            working_dir: None,
        },
    );
    spec = attach_sandbox_request(
        spec,
        output_path.clone(),
        Some("generic".into()),
        vec![],
        None,
    );

    // When
    let handle = SandboxRuntime::spawn(&registry, spec, "sandbox-build")
        .await
        .expect("spawn sandbox build action");
    let exit_code = wait_for_task_exit_code(&registry, &handle.id.0).await;

    // Then
    assert_eq!(exit_code, 0);
    let marker = output_path
        .join("sandbox")
        .join("build-action")
        .join("out.txt");
    let written = std::fs::read_to_string(&marker).expect("out.txt in project_root");
    assert_eq!(written, "built");
}

/// Await the task's actual completion event via its `status_watch()` channel — blocks on the
/// next real status transition rather than sleep-polling against a fixed timeout budget.
async fn wait_for_task_exit_code(registry: &TaskRegistry, task_id: &str) -> i32 {
    use tddy_task::{TaskId, TaskStatus};
    let handle = registry
        .get(&TaskId(task_id.to_string()))
        .await
        .expect("task must be registered before waiting on it");
    let mut status_rx = handle.status_watch();
    loop {
        if status_rx.borrow().is_terminal() {
            break;
        }
        status_rx
            .changed()
            .await
            .expect("status watch closed before task reached a terminal status");
    }
    match handle.status() {
        TaskStatus::Completed { exit_code } => exit_code.unwrap_or(0),
        TaskStatus::Failed { .. } | TaskStatus::Cancelled => -1,
        TaskStatus::Pending | TaskStatus::Running => {
            unreachable!("loop above only exits once status_watch reports a terminal status")
        }
    }
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn sandboxed_action_denies_write_outside_egress() {
    // Given
    let harness = SandboxHarness::new();
    let output_dir = tempfile::tempdir().expect("output dir");
    let output_path = std::fs::canonicalize(output_dir.path()).expect("canonical output_dir");
    let home = std::env::var("HOME").expect("HOME");
    let escape_probe = PathBuf::from(&home).join(".sandbox-action-escape-probe");
    let _ = std::fs::remove_file(&escape_probe);
    let params_json = serde_json::json!({
        "id": "sandbox-escape",
        "command": ["/bin/sh", "-c", format!("touch '{}'", escape_probe.display())],
        "output_dir": output_path.to_string_lossy(),
        "sandbox_recipe": "bash"
    })
    .to_string();

    // When
    let start: StartActionResponse = call_action(
        &harness.action_bridge,
        "StartAction",
        StartActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            session_id: "sandbox-session".to_string(),
            kind: "bash".to_string(),
            params_json,
            sandbox: true,
        },
    )
    .await
    .expect("StartAction must succeed");

    // Then
    let terminal = wait_for_terminal_task(&harness.task_bridge, &start.task_id).await;
    let task = terminal.task.expect("task");
    assert_ne!(task.exit_code, 0, "escape write must fail");
    assert!(
        !escape_probe.exists(),
        "escape probe must not exist at {}",
        escape_probe.display()
    );
}

#[tokio::test]
#[cfg(target_os = "macos")]
async fn sandboxed_bash_pty_action_streams_output() {
    // Given
    let harness = SandboxHarness::new();
    let output_dir = tempfile::tempdir().expect("output dir");
    let output_path = std::fs::canonicalize(output_dir.path()).expect("canonical output_dir");
    let params_json = serde_json::json!({
        "id": "sandbox-pty-bash",
        "command": ["/bin/sh", "-c", "printf pty_ok"],
        "output_dir": output_path.to_string_lossy(),
        "sandbox_recipe": "bash",
        "channel_mode": "pty"
    })
    .to_string();

    // When
    let start: StartActionResponse = call_action(
        &harness.action_bridge,
        "StartAction",
        StartActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            session_id: "sandbox-session".to_string(),
            kind: "bash".to_string(),
            params_json,
            sandbox: true,
        },
    )
    .await
    .expect("StartAction must succeed");

    let terminal = wait_for_terminal_task(&harness.task_bridge, &start.task_id).await;
    let replay = watch_task_replay_bytes(&harness.task_bridge, &start.task_id).await;

    // Then
    let task = terminal.task.expect("task");
    assert_eq!(
        task.exit_code, 0,
        "pty bash must exit 0: {}",
        task.result_json
    );
    let output = String::from_utf8_lossy(&replay);
    assert!(
        output.contains("pty_ok"),
        "pty channel replay must contain pty_ok; got {output:?}"
    );
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[tokio::test]
async fn sandboxed_action_unsupported_off_darwin_linux() {
    // Given
    let harness = SandboxHarness::new();
    let output_dir = tempfile::tempdir().expect("output dir");
    let params_json = serde_json::json!({
        "id": "sandbox-unsupported",
        "command": ["echo", "hi"],
        "output_dir": output_dir.path().to_string_lossy(),
    })
    .to_string();

    // When
    let start: StartActionResponse = call_action(
        &harness.action_bridge,
        "StartAction",
        StartActionRequest {
            session_token: GOOD_TOKEN.to_string(),
            session_id: "sandbox-session".to_string(),
            kind: "bash".to_string(),
            params_json,
            sandbox: true,
        },
    )
    .await
    .expect("StartAction registers task");

    // Then — task fails because platform sandbox is unavailable
    let terminal = wait_for_terminal_task(&harness.task_bridge, &start.task_id).await;
    let task = terminal.task.expect("task");
    assert_eq!(
        task.status,
        TaskStatusProto::TaskStatusFailed as i32,
        "unsupported platform must fail the sandbox task"
    );
}
