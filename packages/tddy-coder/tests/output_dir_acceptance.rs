//! Acceptance test for `--output-dir` (M6): sessions root under explicit output dir.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use std::fs;

fn tddy_coder_bin() -> Command {
    cargo_bin_cmd!("tddy-coder")
}

/// `--output-dir` appears in CLI help.
#[test]
fn cli_accepts_output_dir_flag() {
    let output = tddy_coder_bin()
        .arg("--help")
        .output()
        .expect("run tddy-coder --help");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--output-dir"),
        "--output-dir should appear in help output, stdout: {}",
        stdout
    );
}

/// With `--output-dir`, a new plan session is created under `{output_dir}/sessions/<uuid>/`.
#[test]
#[cfg(unix)]
fn plan_goal_with_output_dir_creates_session_under_output_dir() {
    let tmp = std::env::temp_dir().join(format!("tddy-cli-output-dir-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("create tmp");
    let output_dir = tmp.join("plans-root");

    let mut cmd = tddy_coder_bin();
    cmd.args([
        "--agent",
        "stub",
        "--recipe",
        "tdd",
        "--goal",
        "plan",
        "--prompt",
        "SKIP_QUESTIONS Build auth feature",
        "--output-dir",
        output_dir.to_str().unwrap(),
    ])
    .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "plan with --output-dir should succeed; stdout={} stderr={}",
        stdout,
        stderr
    );

    let sessions_dir = output_dir.join("sessions");
    assert!(
        sessions_dir.is_dir(),
        "expected sessions dir at {}",
        sessions_dir.display()
    );
    let entries: Vec<_> = fs::read_dir(&sessions_dir)
        .expect("read sessions dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "exactly one session dir expected under {}",
        sessions_dir.display()
    );
    let session_dir = entries[0].path();
    assert!(
        session_dir.join("artifacts").join("PRD.md").exists()
            || session_dir.join("PRD.md").exists(),
        "PRD artifact should exist under session dir {}",
        session_dir.display()
    );

    let _ = fs::remove_dir_all(&tmp);
}
