//! One-shot seatbelt spawn inspection — prints diagnostics to stdout (macOS only).

#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use tddy_daemon::sandbox_session::{
    build_allow_read_paths, build_sandbox_runner_env, pick_free_loopback_port,
};
use tddy_sandbox::{format_sandbox_diagnostics, SandboxSpec};
use tddy_sandbox_darwin::{render_profile, spawn};

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

    let tools = tools_binary();
    let grpc_port = pick_free_loopback_port().expect("grpc port");
    let shim_port = pick_free_loopback_port().expect("shim port");
    let ready_marker = project.join("sandbox.ready");

    let runner_argv = vec![
        tools.to_string_lossy().to_string(),
        "sandbox-runner".into(),
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
    let allow_read_paths = build_allow_read_paths(&runner_argv);
    let profile_text = render_profile(&SandboxSpec {
        project_root: project.clone(),
        scratch_dir: scratch.clone(),
        egress_dir: egress.clone(),
        allow_read_paths: allow_read_paths.clone(),
        command: runner_argv.clone(),
        env: env.clone(),
        profile_path: project.join("profile.sb"),
        loopback_allow_ports: vec![grpc_port, shim_port],
        ipc_socket: None,
    })
    .expect("render profile");
    let profile_path = project.join("profile.sb");
    std::fs::write(&profile_path, &profile_text).expect("write profile");

    eprintln!("=== allow_read_paths ({}) ===", allow_read_paths.len());
    for p in &allow_read_paths {
        eprintln!("  {}", p.display());
    }

    eprintln!("profile bytes={}", profile_text.len());

    let probes: [(&str, Vec<String>); 4] = [
        ("echo", vec!["/bin/echo".into(), "hi".into()]),
        (
            "tools-help-direct",
            vec![tools.to_string_lossy().to_string(), "--help".into()],
        ),
        ("tools-help-via-env-i", {
            let mut v = vec!["/usr/bin/env".into(), "-i".into()];
            for (k, val) in &env {
                v.push(format!("{k}={val}"));
            }
            v.push(tools.to_string_lossy().to_string());
            v.push("--help".into());
            v
        }),
        ("runner-via-env-i", {
            let mut v = vec!["/usr/bin/env".into(), "-i".into()];
            for (k, val) in &env {
                v.push(format!("{k}={val}"));
            }
            v.extend(runner_argv.clone());
            v
        }),
    ];

    for (label, argv) in probes {
        let status = Command::new("/usr/bin/sandbox-exec")
            .arg("-f")
            .arg(&profile_path)
            .args(&argv)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .unwrap_or_else(|e| panic!("{label} sandbox-exec failed: {e}"));
        eprintln!(
            "=== probe {label} exit={:?} success={} ===",
            status.code(),
            status.success()
        );
        if let Ok(out) = Command::new("/usr/bin/sandbox-exec")
            .arg("-f")
            .arg(&profile_path)
            .args(&argv)
            .output()
        {
            if !out.stdout.is_empty() {
                let s = String::from_utf8_lossy(&out.stdout);
                eprintln!("  stdout: {}", &s[..200.min(s.len())]);
            }
            if !out.stderr.is_empty() {
                let s = String::from_utf8_lossy(&out.stderr);
                eprintln!("  stderr: {}", &s[..500.min(s.len())]);
            }
        }
    }

    // Minimal profile control: only /usr/bin allow-list (known valid from unit test)
    let minimal_profile = render_profile(&SandboxSpec {
        project_root: project.clone(),
        scratch_dir: project.join(".work"),
        egress_dir: egress.clone(),
        allow_read_paths: vec![PathBuf::from("/usr/bin")],
        command: vec!["/bin/echo".into(), "hi".into()],
        env: Default::default(),
        profile_path: project.join("minimal.sb"),
        loopback_allow_ports: vec![grpc_port, shim_port],
        ipc_socket: None,
    })
    .expect("minimal profile");
    let minimal_path = project.join("minimal.sb");
    std::fs::write(&minimal_path, &minimal_profile).unwrap();
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

    let full_echo = Command::new("/usr/bin/sandbox-exec")
        .arg("-f")
        .arg(&profile_path)
        .arg("/bin/echo")
        .arg("hi")
        .status()
        .unwrap();
    eprintln!(
        "=== full profile echo success={} exit={:?} ===",
        full_echo.success(),
        full_echo.code()
    );

    eprintln!("=== spawn via tddy_sandbox_darwin::spawn ===");
    let mut handle = spawn(SandboxSpec {
        project_root: project.clone(),
        scratch_dir: scratch,
        egress_dir: egress.clone(),
        allow_read_paths,
        command: runner_argv,
        env,
        profile_path: profile_path.clone(),
        loopback_allow_ports: vec![grpc_port, shim_port],
        ipc_socket: None,
    })
    .expect("spawn");

    std::thread::sleep(Duration::from_secs(2));
    let exit = handle.child_mut().try_wait().ok().flatten();
    eprintln!("child try_wait after 2s: {exit:?}");
    eprintln!("{}", format_sandbox_diagnostics(&egress, Some(&project)));
}
