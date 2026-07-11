//! One-shot seatbelt spawn inspection — prints diagnostics to stdout (macOS only).

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use tddy_daemon::sandbox_session::{
    build_sandbox_plan, build_sandbox_runner_env, pick_free_loopback_port, spawn_sandbox_runner,
    SandboxRunnerSpawn,
};
use tddy_sandbox::{format_sandbox_diagnostics, NetworkSpec, SandboxBuilder};
use tddy_sandbox_darwin::render_plan;

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

#[test]
fn sandbox_runner_inspect_seatbelt_spawn() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let scratch = project.join(".work");
    let egress = tmp.path().join("egress");
    let context = project.join("context");
    std::fs::create_dir_all(scratch.join("home")).unwrap();
    std::fs::create_dir_all(scratch.join("tmp")).unwrap();
    std::fs::create_dir_all(&context).unwrap();
    std::fs::create_dir_all(&egress).unwrap();

    let runner = sandbox_runner_binary();
    let tools = tools_binary();
    let grpc_port = pick_free_loopback_port().expect("grpc port");
    let shim_port = pick_free_loopback_port().expect("shim port");
    let ready_marker = project.join("sandbox.ready");
    let profile_path = project.join("profile.sb");

    let runner_argv = vec![
        runner.to_string_lossy().to_string(),
        "--session-id".into(),
        "inspect".into(),
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
        "x".into(),
        "--permission-mode".into(),
        "auto".into(),
        "--grpc-listen-port".into(),
        grpc_port.to_string(),
        "--egress-shim-port".into(),
        shim_port.to_string(),
    ];

    let env = build_sandbox_runner_env(
        &scratch.join("home"),
        &scratch.join("tmp"),
        "inspect",
        &project.join("tool_ipc.sock"),
        &egress,
    );

    let make_params = || SandboxRunnerSpawn {
        project_root: project.clone(),
        scratch_dir: scratch.clone(),
        egress_dir: egress.clone(),
        profile_path: profile_path.clone(),
        runner_argv: runner_argv.clone(),
        env: env.clone(),
        loopback_allow_ports: vec![grpc_port, shim_port],
        ipc_socket: None,
        mounts: vec![],
        host_home: None,
        cgroup: Default::default(),
    };

    let plan = build_sandbox_plan(make_params()).expect("build plan");
    eprintln!("=== plan reads ({}) ===", plan.reads.len());
    for r in &plan.reads {
        eprintln!("  {:?} {}", r.kind, r.host.display());
    }
    let profile_text = render_plan(&plan).expect("render plan");
    std::fs::write(&profile_path, &profile_text).expect("write profile");
    eprintln!("profile bytes={}", profile_text.len());

    let probes: [(&str, Vec<String>); 2] = [
        ("echo", vec!["/bin/echo".into(), "hi".into()]),
        (
            "runner-help",
            vec![runner.to_string_lossy().to_string(), "--help".into()],
        ),
    ];

    for (label, argv) in probes {
        let out = Command::new("/usr/bin/sandbox-exec")
            .arg("-f")
            .arg(&profile_path)
            .args(&argv)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("{label} sandbox-exec failed: {e}"));
        eprintln!(
            "=== probe {label} exit={:?} success={} ===",
            out.status.code(),
            out.status.success()
        );
        if !out.stderr.is_empty() {
            let s = String::from_utf8_lossy(&out.stderr);
            eprintln!("  stderr: {}", &s[..500.min(s.len())]);
        }
    }

    // Minimal control profile: only the OS baseline reads + policy (known valid).
    let minimal_plan = SandboxBuilder::new(
        project.clone(),
        scratch.clone(),
        egress.clone(),
        vec!["/bin/echo".into(), "hi".into()],
    )
    .profile_path(project.join("minimal.sb"))
    .reads(tddy_sandbox::system_baseline_reads())
    .policy(tddy_sandbox_recipes::claude_interactive_policy())
    .network(NetworkSpec::default())
    .build()
    .expect("minimal plan");
    let minimal_path = project.join("minimal.sb");
    std::fs::write(
        &minimal_path,
        render_plan(&minimal_plan).expect("minimal profile"),
    )
    .unwrap();
    let minimal_echo = Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(&minimal_path)
        .arg("/bin/echo")
        .arg("hi")
        .status()
        .unwrap();
    eprintln!(
        "=== minimal profile echo success={} exit={:?} ===",
        minimal_echo.success(),
        minimal_echo.code()
    );

    eprintln!("=== spawn via spawn_sandbox_runner ===");
    let mut handle = spawn_sandbox_runner(make_params()).expect("spawn");

    std::thread::sleep(Duration::from_secs(2));
    let exit = handle.child_mut().try_wait().ok().flatten();
    eprintln!("child try_wait after 2s: {exit:?}");
    eprintln!("{}", format_sandbox_diagnostics(&egress, Some(&project)));
}
