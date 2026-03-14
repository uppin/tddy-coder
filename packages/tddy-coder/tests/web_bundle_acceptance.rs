//! Acceptance tests for web bundle serving (--web-port, --web-bundle-path).
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-03-13-web-bundle-serving.md

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use std::process::Command as StdCommand;
use tddy_core::output::TDDY_SESSIONS_DIR_ENV;

fn tddy_coder_bin() -> Command {
    cargo_bin_cmd!("tddy-coder")
}

/// --web-port without --web-bundle-path exits with a clear error.
#[test]
fn web_port_alone_errors_with_clear_message() {
    let mut cmd = tddy_coder_bin();
    cmd.args(["--agent", "stub", "--web-port", "8080"]);

    let output = cmd.output().expect("run tddy-coder");

    assert!(
        !output.status.success(),
        "should fail when --web-port given without --web-bundle-path"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("web-bundle-path") || stderr.contains("web_bundle_path"),
        "error should mention --web-bundle-path requirement, stderr: {}",
        stderr
    );
}

/// --web-bundle-path without --web-port exits with a clear error.
#[test]
fn web_bundle_path_alone_errors_with_clear_message() {
    let tmp = std::env::temp_dir().join("tddy-web-bundle-path-alone");
    let _ = std::fs::create_dir_all(&tmp);

    let mut cmd = tddy_coder_bin();
    cmd.args([
        "--agent",
        "stub",
        "--web-bundle-path",
        tmp.to_str().unwrap(),
    ]);

    let output = cmd.output().expect("run tddy-coder");

    assert!(
        !output.status.success(),
        "should fail when --web-bundle-path given without --web-port"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("web-port") || stderr.contains("web_port"),
        "error should mention --web-port requirement, stderr: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// --help shows --web-port and --web-bundle-path.
#[test]
fn help_shows_web_port_and_web_bundle_path() {
    let mut cmd = tddy_coder_bin();
    cmd.arg("--help");

    let output = cmd.output().expect("run tddy-coder --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--web-port"),
        "help should document --web-port: {}",
        stdout
    );
    assert!(
        stdout.contains("--web-bundle-path"),
        "help should document --web-bundle-path: {}",
        stdout
    );
}

/// Retries `f` until it returns `Ok` or the timeout (5s) is reached.
/// Yields between attempts to avoid busy-spinning.
fn retry_until_ready<T, E>(mut f: impl FnMut() -> Result<T, E>) -> Result<T, E>
where
    E: std::fmt::Display,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if std::time::Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::yield_now(),
        }
    }
}

/// Kills the child process on drop (e.g. on panic or early return).
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Daemon with --web-port and --web-bundle-path serves index.html at /.
#[test]
#[cfg(unix)]
fn daemon_with_web_flags_serves_index_html_at_root() {
    let tmp = std::env::temp_dir().join("tddy-web-daemon-serve-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    let index_content = "<!DOCTYPE html><html><body>Web Bundle Test</body></html>";
    std::fs::write(tmp.join("index.html"), index_content).expect("write index.html");

    let sessions_base = tmp.join("sessions-base");
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");

    let grpc_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    };
    let web_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    };

    let child = StdCommand::new(assert_cmd::cargo::cargo_bin!("tddy-coder"))
        .env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--daemon",
            "--grpc",
            &grpc_port.to_string(),
            "--web-port",
            &web_port.to_string(),
            "--web-bundle-path",
            tmp.to_str().unwrap(),
        ])
        .spawn()
        .expect("spawn tddy-coder daemon");

    let _guard = KillOnDrop(child);

    let url = format!("http://127.0.0.1:{}/", web_port);
    let body = retry_until_ready(|| reqwest::blocking::get(&url).and_then(|r| r.text()))
        .expect("HTTP GET / within timeout");

    assert!(
        body.contains("Web Bundle Test"),
        "GET / should return index.html content, got: {}",
        body
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
