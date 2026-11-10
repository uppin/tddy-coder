//! Acceptance: a daemon.yaml that still carries a removed agent section fails start-up, naming the
//! `<tddyhome>/tools/<id>.yaml` that replaces it (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`.
//!
//! Start-up reads its configuration through `DaemonConfig::load` before anything else
//! (`src/main.rs`, and the desktop host's `config_source`), so a refusal there is a refusal to
//! start: no logger, no listener, no spawn worker. The refusals are read from this crate's own
//! `tddy-daemon` binary, started with the file as its `--config`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use pretty_assertions::assert_eq;
use tddy_daemon::config::DaemonConfig;

/// A daemon.yaml holding `body` after a valid `users:` mapping, in a directory kept alive by the
/// returned guard.
fn a_daemon_yaml(body: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, body).unwrap();
    std::fs::write(
        dir.path().join("users.yaml"),
        "- github_user: dev\n  os_user: dev\n",
    )
    .unwrap();
    (dir, path)
}

/// How long a refused start-up may take to exit: it fails before the logger, the spawn worker or
/// any listener exists, so anything near this means the daemon started instead.
const STARTUP_REFUSAL_DEADLINE: Duration = Duration::from_secs(30);

/// Why the `tddy-daemon` binary refused to start with the daemon.yaml at `path`: the error it
/// exits with, as `main` reports it (`Error: <message>` on stderr).
fn the_startup_refusal(path: &Path) -> String {
    let mut daemon = Command::new(env!("CARGO_BIN_EXE_tddy-daemon"))
        .arg("--config")
        .arg(path)
        .env_remove("TDDY_DAEMON_CONFIG")
        // `main` returns an `anyhow::Error`; with backtraces on (CI sets `RUST_BACKTRACE=1`) its
        // report would carry a stack trace after the message.
        .env("RUST_BACKTRACE", "0")
        .env("RUST_LIB_BACKTRACE", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start tddy-daemon");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = daemon.try_wait().expect("poll tddy-daemon") {
            break status;
        }
        if started.elapsed() > STARTUP_REFUSAL_DEADLINE {
            let _ = daemon.kill();
            let _ = daemon.wait();
            panic!("tddy-daemon was still running after {STARTUP_REFUSAL_DEADLINE:?}: start-up must refuse a removed agent section");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let output = daemon.wait_with_output().expect("tddy-daemon's stderr");
    assert!(
        !status.success(),
        "start-up must refuse a removed agent section"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .trim()
        .strip_prefix("Error: ")
        .unwrap_or_else(|| panic!("tddy-daemon's error, as main reports it: {stderr:?}"))
        .to_string()
}

#[test]
fn a_daemon_yaml_with_claude_cli_fails_startup_naming_tddyhome_tools_claude_yaml() {
    // Given
    let (_dir, path) = a_daemon_yaml("claude_cli:\n  binary_path: /opt/claude/bin/claude\n");

    // When
    let refusal = the_startup_refusal(&path);

    // Then
    assert_eq!(
        refusal,
        format!(
            "{}: daemon.yaml `claude_cli:` is removed: moved to <tddyhome>/tools/claude.yaml \
             (binary_path → binary.command)",
            path.display()
        )
    );
}

#[test]
fn the_same_for_cursor_cli_and_cli_harnesses() {
    // Given
    let (_dir, path) = a_daemon_yaml(
        "cursor_cli:\n  binary_path: /opt/cursor/agent\ncli_harnesses:\n  opencode-cli:\n    binary_path: /opt/opencode\n",
    );

    // When
    let refusal = the_startup_refusal(&path);

    // Then
    assert_eq!(
        refusal,
        format!(
            "{}: daemon.yaml `cursor_cli:` is removed: moved to <tddyhome>/tools/cursor.yaml \
             (binary_path → binary.command); daemon.yaml `cli_harnesses.opencode-cli:` is \
             removed: moved to <tddyhome>/tools/opencode.yaml (binary_path → binary.command)",
            path.display()
        )
    );
}

#[test]
fn a_daemon_yaml_without_an_agent_section_still_starts() {
    // Given — a positive control: the refusal is about the removed sections, not about the file
    let (_dir, path) = a_daemon_yaml("allowed_agents:\n  - id: claude\n");

    // When
    let loaded = DaemonConfig::load(&path).map(|config| config.allowed_agents.len());

    // Then
    assert_eq!(loaded.map_err(|e| e.to_string()), Ok(1));
}
