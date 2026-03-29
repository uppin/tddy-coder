//! Acceptance: remote exit codes and PTY bash smoke (PRD: shell_remote_exit_code_propagation, shell_pty_bash_smoke).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use reqwest::blocking::Client;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("tddy-remote manifest under packages/<crate>/")
        .to_path_buf()
}

fn daemon_bin() -> PathBuf {
    let rel = std::env::var("CARGO_BIN_EXE_tddy-daemon")
        .unwrap_or_else(|_| "target/debug/tddy-daemon".to_string());
    let p = workspace_root().join(&rel);
    if p.exists() {
        p
    } else {
        PathBuf::from(rel)
    }
}

fn pick_free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    l.local_addr().expect("addr").port()
}

fn write_minimal_bundle(dir: &Path) {
    std::fs::write(dir.join("index.html"), "<!doctype html><title>tddy</title>")
        .expect("index.html");
}

fn write_daemon_config(path: &Path, port: u16, bundle: &Path) {
    let yaml = format!(
        r#"
listen:
  web_port: {port}
  web_host: "127.0.0.1"
web_bundle_path: {bundle}
github:
  stub: true
  client_id: "acceptance-client"
users:
  - github_user: "acceptance-user"
    os_user: "acceptance-os"
"#,
        port = port,
        bundle = bundle.display()
    );
    std::fs::write(path, yaml).expect("daemon.yaml");
}

struct DaemonProcess {
    child: Child,
    base_url: String,
}

fn spawn_daemon() -> DaemonProcess {
    let bin = daemon_bin();
    assert!(
        bin.exists(),
        "tddy-daemon binary missing at {} — run `cargo build -p tddy-daemon` from the workspace root",
        bin.display()
    );

    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle = tmp.path().join("bundle");
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    write_minimal_bundle(&bundle);

    let port = pick_free_port();
    let cfg_path = tmp.path().join("daemon.yaml");
    write_daemon_config(&cfg_path, port, &bundle);

    let mut child = Command::new(&bin)
        .arg("-c")
        .arg(&cfg_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tddy-daemon");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client");

    for _ in 0..40 {
        if let Ok(resp) = client.get(format!("{base_url}/api/config")).send() {
            if resp.status().is_success() {
                return DaemonProcess { child, base_url };
            }
        }
        thread::sleep(Duration::from_millis(150));
    }

    let _ = child.kill();
    panic!("tddy-daemon did not become ready at {base_url}");
}

fn write_remote_config(dir: &std::path::Path, connect_base_url: &str) -> std::path::PathBuf {
    let p = dir.join("remote.yaml");
    std::fs::write(
        &p,
        format!(
            r#"authorities:
  - id: "acceptance-host"
    connect_base_url: "{connect_base_url}"
"#,
            connect_base_url = connect_base_url
        ),
    )
    .expect("write config");
    p
}

#[test]
fn shell_remote_exit_code_propagation() {
    let DaemonProcess { child, base_url } = spawn_daemon();
    let _guard = scopeguard::guard(child, |mut c| {
        let _ = c.kill();
        let _ = c.wait();
    });

    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = write_remote_config(dir.path(), &base_url);
    let cfg_s = cfg.to_str().expect("utf8");

    let mut false_cmd = cargo_bin_cmd!("tddy-remote");
    false_cmd.args(["--config", cfg_s, "exec", "acceptance-host", "false"]);
    false_cmd
        .assert()
        .code(predicate::ne(0))
        .stderr(predicate::str::is_empty().not());

    let mut true_cmd = cargo_bin_cmd!("tddy-remote");
    true_cmd.args(["--config", cfg_s, "exec", "acceptance-host", "true"]);
    true_cmd.assert().code(0);
}

#[test]
fn shell_pty_bash_smoke() {
    let DaemonProcess { child, base_url } = spawn_daemon();
    let _guard = scopeguard::guard(child, |mut c| {
        let _ = c.kill();
        let _ = c.wait();
    });

    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = write_remote_config(dir.path(), &base_url);
    let mut cmd = cargo_bin_cmd!("tddy-remote");
    cmd.args([
        "--config",
        cfg.to_str().expect("utf8"),
        "shell",
        "--pty",
        "acceptance-host",
    ]);
    cmd.timeout(std::time::Duration::from_secs(8));
    cmd.assert().success().stdout(
        predicate::str::is_match(r"(?i)(\$\s|#\s|\[.*@.*\].*[$#]\s|\bbash\b)").expect("regex"),
    );
}
