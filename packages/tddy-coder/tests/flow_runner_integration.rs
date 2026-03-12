//! Integration tests for FlowRunner-based plan execution.
//!
//! Verifies plan goal produces plan output via WorkflowEngine (graph-flow path).
//!
//! Uses tddy-coder --agent stub (StubBackend) with SKIP_QUESTIONS so the test does not need clarification input.
//! Runs as subprocess with piped stdin ("a\n" for plan approval) to avoid blocking.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use std::path::PathBuf;

fn temp_output_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("tddy-flowrunner-plan-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create output dir");
    dir
}

/// Plan goal produces a plan directory when given feature input.
/// Uses tddy-coder --agent stub (StubBackend) with piped stdin for plan approval.
/// TDDY_SESSIONS_DIR set to temp dir so tests do not write to production ~/.tddy.
#[test]
#[cfg(unix)]
fn run_plan_via_flow_runner_produces_plan_directory() {
    let sessions_base = temp_output_dir();
    let sessions_base_str = sessions_base.to_str().expect("path");

    let mut cmd = cargo_bin_cmd!("tddy-coder");
    cmd.env(tddy_core::output::TDDY_SESSIONS_DIR_ENV, sessions_base_str)
        .args([
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "Add user authentication SKIP_QUESTIONS",
        ])
        .write_stdin("a\n");

    cmd.assert().success();

    let sessions_dir = sessions_base.join(tddy_core::output::SESSIONS_SUBDIR);
    let plan_dir = std::fs::read_dir(&sessions_dir)
        .expect("sessions dir should exist")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .expect("at least one session dir should exist");
    assert!(
        plan_dir.is_dir(),
        "plan_dir should be a directory: {}",
        plan_dir.display()
    );
    let prd_path = plan_dir.join("PRD.md");
    assert!(
        prd_path.exists(),
        "PRD.md should exist in plan_dir {}; contents: {:?}",
        plan_dir.display(),
        std::fs::read_dir(&plan_dir).ok().map(|d| d
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect::<Vec<_>>())
    );
    // TODO content is merged into PRD.md as last section; no separate TODO.md
    let prd_content = std::fs::read_to_string(&prd_path).expect("read PRD.md");
    assert!(
        prd_content.contains("- [ ]") || prd_content.contains("## TODO"),
        "PRD.md should contain TODO content (as last section); got: {}",
        &prd_content[..prd_content.len().min(500)]
    );

    let _ = std::fs::remove_dir_all(&sessions_base);
}
