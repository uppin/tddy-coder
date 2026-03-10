//! Integration tests for FlowRunner-based plan execution.
//!
//! Verifies plan goal produces plan output via WorkflowEngine (graph-flow path).
//!
//! Uses tddy-demo (StubBackend) with SKIP_QUESTIONS so the test does not need clarification input.
//! Runs as subprocess with piped stdin ("a\n" for plan approval) to avoid blocking.

use assert_cmd::Command;
use std::path::PathBuf;

fn temp_output_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("tddy-flowrunner-plan-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create output dir");
    dir
}

/// Plan goal produces a plan directory when given feature input.
/// Uses tddy-demo binary (StubBackend) with piped stdin for plan approval.
#[test]
fn run_plan_via_flow_runner_produces_plan_directory() {
    let output_dir = temp_output_dir();
    let output_dir_str = output_dir.to_str().expect("path");

    let mut cmd = Command::cargo_bin("tddy-demo").expect("tddy-demo binary");
    cmd.args([
        "--goal",
        "plan",
        "--output-dir",
        output_dir_str,
        "--prompt",
        "Add user authentication SKIP_QUESTIONS",
    ])
    .write_stdin("a\n");

    cmd.assert().success();

    let plan_dir_name =
        tddy_core::output::slugify_directory_name("Add user authentication SKIP_QUESTIONS");
    let plan_dir = output_dir.join(&plan_dir_name);
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

    let _ = std::fs::remove_dir_all(&output_dir);
}
