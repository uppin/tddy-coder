//! Integration tests for FlowRunner-based plan execution.
//!
//! Verifies run_plan_via_flow_runner exists and produces plan output when CLI/TUI
//! migrates from Workflow to FlowRunner.
//!
//! Uses SKIP_QUESTIONS in prompt because FlowRunner does not support clarification input.

use std::path::PathBuf;
use tddy_coder::{run_plan_via_flow_runner, Args};
use tddy_core::{SharedBackend, StubBackend};

fn temp_output_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("tddy-flowrunner-plan-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create output dir");
    dir
}

/// run_plan_via_flow_runner produces a plan directory when given feature input.
#[test]
fn run_plan_via_flow_runner_produces_plan_directory() {
    let output_dir = temp_output_dir();
    let args = Args {
        goal: Some("plan".to_string()),
        output_dir: output_dir.clone(),
        plan_dir: None,
        conversation_output: None,
        model: None,
        allowed_tools: None,
        debug: false,
        debug_output: None,
        agent: "stub".to_string(),
        prompt: Some("Add user authentication SKIP_QUESTIONS".to_string()),
        grpc: None,
    };

    let backend: SharedBackend =
        SharedBackend::from_any(tddy_core::AnyBackend::Stub(StubBackend::new()));

    let plan_dir =
        run_plan_via_flow_runner(&args, backend).expect("run_plan_via_flow_runner should succeed");

    assert!(
        plan_dir.is_dir(),
        "plan_dir should be a directory: {}",
        plan_dir.display()
    );
    assert!(
        plan_dir.join("PRD.md").exists(),
        "PRD.md should exist in plan_dir"
    );
    assert!(
        plan_dir.join("TODO.md").exists(),
        "TODO.md should exist in plan_dir"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}
