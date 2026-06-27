//! Smoke: sandbox-runner reaches ready marker inside Seatbelt (macOS only).

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::time::Duration;

use tddy_daemon::sandbox_session::{build_allow_read_paths, pick_free_loopback_port, spawn_sandbox_runner};
use tddy_sandbox::format_sandbox_diagnostics;
use tddy_sandbox_darwin::render_profile;
use tddy_sandbox::SandboxSpec;

fn tools_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-tools")
        })
}

fn demo_tui_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-demo-tui")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-demo-tui")
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

    let tools = tools_binary();
    let demo = demo_tui_binary();
    assert!(tools.exists(), "build tddy-tools first");
    assert!(demo.exists(), "build tddy-demo-tui first");

    let grpc_port = pick_free_loopback_port().expect("grpc port");
    let shim_port = pick_free_loopback_port().expect("shim port");
    let ready_marker = project.join("sandbox.ready");

    let runner_argv = vec![
        tools.to_string_lossy().to_string(),
        "sandbox-runner".into(),
        "--session-id".into(),
        "spawn-smoke".into(),
        "--context-dir".into(),
        context.to_string_lossy().to_string(),
        "--grpc-socket".into(),
        project.join("sandbox.grpc.sock").to_string_lossy().to_string(),
        "--tool-ipc-socket".into(),
        project.join("tool_ipc.sock").to_string_lossy().to_string(),
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

    let mut env = tddy_daemon::sandbox_session::build_sandbox_runner_env(
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
    let allow_read_paths = build_allow_read_paths(&runner_argv);
    let profile_text = render_profile(&SandboxSpec {
        project_root: project.clone(),
        scratch_dir: scratch.clone(),
        egress_dir: egress.clone(),
        allow_read_paths: allow_read_paths.clone(),
        command: runner_argv.clone(),
        env: env.clone(),
        profile_path: project.join("profile.sb"),
        loopback_allow_ports: loopback_allow_ports.clone(),
        ipc_socket: None,
    })
    .expect("render profile");
    let profile_path = project.join("profile.sb");
    std::fs::write(&profile_path, &profile_text).expect("write profile");
    let echo_check = std::process::Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(&profile_path)
        .arg("/bin/echo")
        .arg("hi")
        .status()
        .expect("sandbox-exec echo");
    assert!(
        echo_check.code() != Some(6),
        "profile must be valid before spawn\nallow_read_paths={allow_read_paths:#?}\n--- profile ---\n{profile_text}"
    );
    for (label, args) in [
        ("tools-help", vec![tools.to_string_lossy().to_string(), "--help".into()]),
        (
            "runner-help",
            vec![
                tools.to_string_lossy().to_string(),
                "sandbox-runner".into(),
                "--help".into(),
            ],
        ),
    ] {
        let mut cmd = std::process::Command::new("/usr/bin/sandbox-exec");
        cmd.arg("-f").arg(&profile_path).arg(&args[0]);
        cmd.args(&args[1..]);
        let status = cmd.status().expect("sandbox-exec subcommand");
        assert!(
            status.code() != Some(6),
            "{label} must not abort inside sandbox (status={status:?})"
        );
    }

    // When
    let mut handle = spawn_sandbox_runner(
        project.clone(),
        scratch,
        egress.clone(),
        profile_path,
        runner_argv,
        env,
        loopback_allow_ports,
        None,
    )
    .expect("spawn sandbox-runner");

    let deadline = Duration::from_secs(15);
    let start = std::time::Instant::now();
    while start.elapsed() < deadline {
        if ready_marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Then
    let exit_status = handle.child_mut().try_wait().ok().flatten();
    assert!(
        ready_marker.exists(),
        "ready marker must appear (child={exit_status:?})\n{}\npid={}",
        format_sandbox_diagnostics(&egress, Some(&project)),
        handle.pid()
    );

    let _ = handle.into_child().kill();
}
