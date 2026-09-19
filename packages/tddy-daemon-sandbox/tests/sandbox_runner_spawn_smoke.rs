//! Smoke: sandbox-runner reaches ready marker inside Seatbelt (macOS only).

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::time::Duration;

use tddy_daemon_sandbox::sandbox_session::{
    pick_free_loopback_port, spawn_sandbox_runner, SandboxRunnerSpawn,
};
use tddy_sandbox::{format_sandbox_diagnostics, SandboxHandle};

/// A spawned jail that is torn down on every exit from the test, a failed assertion included.
///
/// [`SandboxHandle`] is a plain struct over [`std::process::Child`], which neither kills nor reaps
/// on drop, so dropping a handle is not teardown. Teardown placed on the last line of a test is
/// not teardown either: the `ready_marker` assertions above it panic exactly when the jail failed
/// to boot, which is the case that orphans a `tddy-sandbox-runner` onto the host for as long as
/// the machine is up. `wait()` follows the `kill()` so the reaped pid does not linger as a zombie
/// on the happy path either.
struct SpawnedJail {
    handle: SandboxHandle,
}

impl SpawnedJail {
    fn new(handle: SandboxHandle) -> Self {
        Self { handle }
    }

    fn pid(&self) -> u32 {
        self.handle.pid()
    }

    fn handle_mut(&mut self) -> &mut SandboxHandle {
        &mut self.handle
    }
}

impl Drop for SpawnedJail {
    fn drop(&mut self) {
        self.handle.child_mut().kill().ok();
        self.handle.child_mut().wait().ok();
    }
}

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

/// **sandbox_runner_writes_ready_marker_inside_seatbelt**: confined sandbox-runner binds
/// loopback ports and writes the gRPC ready marker.
#[tokio::test]
async fn sandbox_runner_writes_ready_marker_inside_seatbelt() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(project.join(".work").join("home")).unwrap();
    std::fs::create_dir_all(project.join(".work").join("tmp")).unwrap();
    std::fs::create_dir_all(project.join("context")).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    // Canonicalize once the dirs exist: TMPDIR is reached via the /tmp -> /private/tmp
    // symlink, and Seatbelt matches the socket bind path against the (canonical) SBPL
    // rules. The daemon does the same in connection_service; without it the in-jail
    // tool-IPC AF_UNIX bind fails with "Operation not permitted".
    let project = std::fs::canonicalize(&project).unwrap();
    let egress = std::fs::canonicalize(&egress).unwrap();
    let scratch = project.join(".work");
    let context = project.join("context");

    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    assert!(runner.exists(), "build tddy-sandbox-runner first");
    assert!(tools.exists(), "build tddy-tools first");

    let grpc_port = pick_free_loopback_port().expect("grpc port");
    let shim_port = pick_free_loopback_port().expect("shim port");
    let ready_marker = project.join("sandbox.ready");

    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        "spawn-smoke".into(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--grpc-socket".into(),
        project
            .join("sandbox.grpc.sock")
            .to_string_lossy()
            .to_string(),
        "--tool-ipc-socket".into(),
        project.join("tool_ipc.sock").to_string_lossy().to_string(),
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
        "--grpc-listen-port".into(),
        grpc_port.to_string(),
        "--egress-shim-port".into(),
        shim_port.to_string(),
    ];

    let mut env = tddy_daemon_sandbox::sandbox_session::build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        "spawn-smoke",
        &project.join("tool_ipc.sock"),
        &egress,
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress.to_string_lossy().to_string(),
    );

    let loopback_allow_ports = vec![grpc_port, shim_port];
    let profile_path = project.join("profile.sb");

    // When
    let mut jail = SpawnedJail::new(
        spawn_sandbox_runner(SandboxRunnerSpawn {
            project_root: project.clone(),
            scratch_dir: scratch,
            egress_dir: egress.clone(),
            profile_path,
            runner_argv,
            env,
            loopback_allow_ports,
            ipc_socket: None,
            mounts: vec![],
            host_home: None,
            cgroup: Default::default(),
        })
        .expect("spawn sandbox-runner"),
    );

    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if ready_marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Then
    let exit_status = jail.handle_mut().child_mut().try_wait().ok().flatten();
    assert!(
        ready_marker.exists(),
        "ready marker must appear (child={exit_status:?})\n{}\npid={}",
        format_sandbox_diagnostics(&egress, Some(&project)),
        jail.pid()
    );
}

/// **generic_pty_runner_writes_ready_marker_inside_seatbelt**: confined sandbox-runner in generic
/// PTY mode (`--pty-command`) still binds loopback gRPC and writes the ready marker.
#[tokio::test]
async fn generic_pty_runner_writes_ready_marker_inside_seatbelt() {
    // Given
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(project.join(".work").join("home")).unwrap();
    std::fs::create_dir_all(project.join(".work").join("tmp")).unwrap();
    std::fs::create_dir_all(project.join("context")).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    let project = std::fs::canonicalize(&project).unwrap();
    let egress = std::fs::canonicalize(&egress).unwrap();
    let scratch = project.join(".work");
    let context = project.join("context");

    let runner = sandbox_runner_binary();
    assert!(runner.exists(), "build tddy-sandbox-runner first");

    let grpc_port = pick_free_loopback_port().expect("grpc port");
    let shim_port = pick_free_loopback_port().expect("shim port");
    let ready_marker = project.join("sandbox.ready");

    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        "generic-pty-smoke".into(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--grpc-socket".into(),
        project
            .join("sandbox.grpc.sock")
            .to_string_lossy()
            .to_string(),
        "--tool-ipc-socket".into(),
        project.join("tool_ipc.sock").to_string_lossy().to_string(),
        "--ready-marker".into(),
        ready_marker.to_string_lossy().to_string(),
        "--grpc-listen-port".into(),
        grpc_port.to_string(),
        "--egress-shim-port".into(),
        shim_port.to_string(),
        "--model".into(),
        String::new(),
        "--pty-command=/bin/sleep".into(),
        "--pty-command=30".into(),
    ];

    let mut env = tddy_daemon_sandbox::sandbox_session::build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        "generic-pty-smoke",
        &project.join("tool_ipc.sock"),
        &egress,
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress.to_string_lossy().to_string(),
    );

    // When
    let _jail = SpawnedJail::new(
        spawn_sandbox_runner(SandboxRunnerSpawn {
            project_root: project.clone(),
            scratch_dir: scratch,
            egress_dir: egress.clone(),
            profile_path: project.join("profile.sb"),
            runner_argv,
            env,
            loopback_allow_ports: vec![grpc_port, shim_port],
            ipc_socket: None,
            mounts: vec![],
            host_home: None,
            cgroup: Default::default(),
        })
        .expect("spawn generic pty sandbox-runner"),
    );

    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if ready_marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Then
    assert!(
        ready_marker.exists(),
        "generic pty ready marker must appear\n{}",
        format_sandbox_diagnostics(&egress, Some(&project))
    );
}

/// **generic_pty_host_relay_streams_command_output**: generic PTY mode forwards bytes to the host relay.
#[tokio::test]
async fn generic_pty_host_relay_streams_command_output() {
    use bytes::Bytes;
    use tddy_daemon_sandbox::sandbox_session::connect_sandbox_session_client;
    use tddy_sandbox_runner::{run_host_relay, HostRelayConfig, NullToolHandler};
    use tokio::sync::mpsc;

    // Given
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let egress = tmp.path().join("egress");
    std::fs::create_dir_all(project.join(".work").join("home")).unwrap();
    std::fs::create_dir_all(project.join(".work").join("tmp")).unwrap();
    std::fs::create_dir_all(project.join("context")).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    let project = std::fs::canonicalize(&project).unwrap();
    let egress = std::fs::canonicalize(&egress).unwrap();
    let scratch = project.join(".work");
    let context = project.join("context");

    let runner = sandbox_runner_binary();
    assert!(runner.exists(), "build tddy-sandbox-runner first");

    let grpc_port = pick_free_loopback_port().expect("grpc port");
    let shim_port = pick_free_loopback_port().expect("shim port");
    let ready_marker = project.join("sandbox.ready");
    let grpc_socket = project.join("sandbox.grpc.sock");

    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        "generic-pty-relay".into(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--grpc-socket".into(),
        grpc_socket.to_string_lossy().to_string(),
        "--tool-ipc-socket".into(),
        project.join("tool_ipc.sock").to_string_lossy().to_string(),
        "--ready-marker".into(),
        ready_marker.to_string_lossy().to_string(),
        "--grpc-listen-port".into(),
        grpc_port.to_string(),
        "--egress-shim-port".into(),
        shim_port.to_string(),
        "--model".into(),
        String::new(),
        "--pty-command=/bin/sh".into(),
        "--pty-command=-c".into(),
        "--pty-command=printf pty_ok".into(),
    ];

    let mut env = tddy_daemon_sandbox::sandbox_session::build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        "generic-pty-relay",
        &project.join("tool_ipc.sock"),
        &egress,
    );
    env.insert(
        "TDDY_SANDBOX_EGRESS_DIR".into(),
        egress.to_string_lossy().to_string(),
    );

    let _jail = SpawnedJail::new(
        spawn_sandbox_runner(SandboxRunnerSpawn {
            project_root: project.clone(),
            scratch_dir: scratch,
            egress_dir: egress.clone(),
            profile_path: project.join("profile.sb"),
            runner_argv,
            env,
            loopback_allow_ports: vec![grpc_port, shim_port],
            ipc_socket: None,
            mounts: vec![],
            host_home: None,
            cgroup: Default::default(),
        })
        .expect("spawn generic pty sandbox-runner"),
    );

    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if ready_marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ready_marker.exists(), "ready marker must exist");

    let client = connect_sandbox_session_client(&ready_marker, &grpc_socket)
        .await
        .expect("connect sandbox grpc");
    let (term_tx, mut term_rx) = mpsc::unbounded_channel::<Bytes>();
    let relay = run_host_relay(
        client,
        NullToolHandler,
        HostRelayConfig::new("generic-pty-relay", term_tx),
        mpsc::unbounded_channel().1,
    )
    .await
    .expect("start host relay");

    let captured = tokio::time::timeout(Duration::from_secs(5), async {
        let mut buf = Vec::new();
        while let Some(chunk) = term_rx.recv().await {
            buf.extend_from_slice(&chunk);
            if String::from_utf8_lossy(&buf).contains("pty_ok") {
                return buf;
            }
        }
        buf
    })
    .await
    .expect("terminal output timeout");

    // The relay task lives as long as the jail's `SessionChannel` is open, and the runner outlives
    // the PTY command it ran, so awaiting the handle here waits for a jail that is never going to
    // close its channel. What the test claims — that the relay forwarded the PTY bytes — is
    // already settled by `captured` above; the relay is simply stopped so the task does not
    // outlive the test that started it.
    relay.abort();

    // Then
    assert!(
        String::from_utf8_lossy(&captured).contains("pty_ok"),
        "relay must capture pty output; got {:?}",
        String::from_utf8_lossy(&captured)
    );
}
