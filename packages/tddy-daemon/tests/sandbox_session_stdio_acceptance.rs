//! Acceptance: `tddy-daemon`'s real sandboxed-session lifecycle must drive
//! `tddy-sandbox-runner` entirely over `tddy-stdio` — not gRPC — for every real session (see
//! docs/dev/TODO.md, "Switch `tddy-daemon`'s real session lifecycle onto the stdio transport").
//!
//! `sandbox_stdio_seatbelt_acceptance.rs` already proves the lower-level primitives
//! (`bridge_sandbox_stdio`, `StdioSandboxClient`, `run_host_relay`) end-to-end through a real
//! Seatbelt jail, wired together by hand. This file goes one level higher: it drives the actual
//! production `sandbox_session::dial_and_bridge` — the exact function
//! `connection_service.rs`'s spawn/dial orchestration calls for every real session — and asserts
//! the daemon's own spawn argv no longer builds any gRPC control-channel flags.

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tddy_daemon::sandbox_session::{
    build_sandbox_runner_env, dial_and_bridge, pick_free_loopback_port, spawn_sandbox_runner,
    SandboxRunnerSpawn,
};
use tokio::sync::{broadcast, mpsc};

const CALL_TIMEOUT: Duration = Duration::from_secs(8);

fn sandbox_runner_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-sandbox-runner")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-sandbox-runner")
        })
}

fn tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

/// **real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio**: spawns a
/// real Seatbelt jail with `tddy-sandbox-runner --stdio` and no gRPC flags anywhere in its argv,
/// then calls the production `dial_and_bridge` — the exact function
/// `connection_service.rs`'s spawn/dial orchestration calls for every real sandboxed session —
/// and dispatches a real `Read` tool call through it, exactly as an MCP tool call issued from
/// inside the jail would be. The real file content coming back (not a fake handler's marker
/// result, as the lower-level `run_host_relay` primitive test uses) proves the whole
/// daemon-session chain — `dial_and_bridge`'s own dial, the real `DaemonToolHandler`,
/// `tool_engine::execute_tool` — runs with zero gRPC on the control channel.
#[tokio::test]
async fn real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio() {
    // Given a worktree with a real file, and a Seatbelt-jailed sandbox-runner spawned with only
    // `--stdio` — no `--grpc-socket`/`--grpc-listen-port`/`--grpc-uds` anywhere in its argv
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(project.join(".work").join("home")).unwrap();
    std::fs::create_dir_all(project.join(".work").join("tmp")).unwrap();
    std::fs::create_dir_all(project.join("context")).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    // Canonicalize once the dirs exist: TMPDIR is reached via the /tmp -> /private/tmp symlink,
    // and Seatbelt matches the socket bind path against the (canonical) SBPL rules — same
    // reasoning as `sandbox_stdio_seatbelt_acceptance.rs`.
    let project = std::fs::canonicalize(&project).unwrap();
    let egress = std::fs::canonicalize(&egress).unwrap();
    let scratch = project.join(".work");
    let context = project.join("context");
    let worktree = project.join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(worktree.join("README.md"), "hello from the real worktree\n").unwrap();

    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    assert!(runner.exists(), "build tddy-sandbox-runner first");
    assert!(tools.exists(), "build tddy-tools first");

    let ready_marker = project.join("sandbox.ready");
    let tool_ipc_socket = project.join("tool_ipc.sock");
    let session_id = "daemon-stdio-session";
    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        session_id.to_string(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--tool-ipc-socket".into(),
        tool_ipc_socket.to_string_lossy().to_string(),
        "--tddy-tools-path".into(),
        tools.to_string_lossy().to_string(),
        "--ready-marker".into(),
        ready_marker.to_string_lossy().to_string(),
        "--claude-binary".into(),
        "/bin/sleep".into(),
        "--model".into(),
        "claude-opus-4-8".into(),
        "--permission-mode".into(),
        "auto".into(),
        "--stdio".into(),
    ];

    let mut env = build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        session_id,
        &tool_ipc_socket,
        &egress,
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress.to_string_lossy().to_string(),
    );

    let shim_port = pick_free_loopback_port().expect("egress shim port");
    let profile_path = project.join("profile.sb");

    let mut handle = spawn_sandbox_runner(SandboxRunnerSpawn {
        project_root: project.clone(),
        scratch_dir: scratch,
        egress_dir: egress,
        profile_path,
        runner_argv,
        env,
        loopback_allow_ports: vec![shim_port],
        ipc_socket: None,
        mounts: vec![],
        host_home: None,
        cgroup: Default::default(),
    })
    .expect("spawn sandbox-runner");

    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if ready_marker.exists() {
            break;
        }
        if let Some(reason) = handle.try_exit_diagnostic() {
            panic!("sandbox child died before ready marker: {reason}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready_marker.exists(), "ready marker must appear");

    // When driving the production `dial_and_bridge` over the jailed process's piped stdio — the
    // same call `connection_service.rs` makes for a real session, once switched off gRPC
    let (stdout_tx, _stdout_rx) = broadcast::channel(16);
    let capture = Arc::new(StdMutex::new(Vec::new()));
    let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel();
    let task_registry = tddy_task::TaskRegistry::default();

    tokio::time::timeout(
        CALL_TIMEOUT,
        dial_and_bridge(
            session_id,
            worktree.clone(),
            &mut handle,
            task_registry,
            stdout_tx,
            capture,
            stdin_rx,
            Arc::new(Vec::new()),
        ),
    )
    .await
    .expect("dial_and_bridge timed out")
    .expect("dial_and_bridge over stdio");

    // Then a real `Read` tool call, dispatched from inside the jail exactly as a real MCP tool
    // call would be, returns the actual worktree file content it was bridged to
    let ipc_result = tokio::time::timeout(
        CALL_TIMEOUT,
        tddy_tools::session_tool_client::dispatch_via_sandbox_ipc(
            &tool_ipc_socket,
            "Read",
            &serde_json::json!({"path": "README.md"}),
        ),
    )
    .await
    .expect("tool dispatch timed out");

    let parsed: serde_json::Value = serde_json::from_str(&ipc_result).expect("valid json response");
    assert_eq!(
        parsed.get("content").and_then(|v| v.as_str()),
        Some("hello from the real worktree\n"),
        "expected the real worktree file content to round-trip through dial_and_bridge: {parsed}"
    );

    handle.child_mut().kill().ok();
    handle.child_mut().wait().ok();
}

/// **sandboxed_session_spawn_argv_carries_stdio_and_no_grpc_flags**: the daemon's sandboxed-session
/// spawn/dial orchestration in `connection_service.rs` must request the stdio transport and must
/// never build the gRPC control-channel flags — per this repo's convention, once a call site
/// switches to `tddy-stdio` its old transport is deleted outright (no dual-path fallback).
#[test]
fn sandboxed_session_spawn_argv_carries_stdio_and_no_grpc_flags() {
    // Given
    let connection_service_rs = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/connection_service.rs"
    ));

    // When / Then — the sandbox-runner spawn argv must request the stdio transport…
    assert!(
        connection_service_rs.contains("\"--stdio\""),
        "sandbox-runner spawn argv must pass --stdio"
    );
    // …and must never build any of the gRPC control-channel flags for that spawn.
    for grpc_flag in [
        "\"--grpc-socket\"",
        "\"--grpc-listen-port\"",
        "\"--grpc-uds\"",
    ] {
        assert!(
            !connection_service_rs.contains(grpc_flag),
            "sandbox-runner spawn argv must not pass {grpc_flag} once switched to stdio"
        );
    }
}
