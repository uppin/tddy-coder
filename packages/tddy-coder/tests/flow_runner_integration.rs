//! Integration tests for FlowRunner-based plan execution.
//!
//! Verifies plan goal produces plan output via WorkflowEngine (graph-flow path).
//!
//! Uses SKIP_QUESTIONS in prompt because the test does not provide clarification input.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tddy_coder::{run_with_args, Args};

fn temp_output_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("tddy-flowrunner-plan-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create output dir");
    dir
}

/// Plan goal produces a plan directory when given feature input (via WorkflowEngine).
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

    run_with_args(&args, std::sync::Arc::new(AtomicBool::new(false)))
        .expect("run_with_args plan should succeed");

    let plan_dir_name =
        tddy_core::output::slugify_directory_name("Add user authentication SKIP_QUESTIONS");
    let plan_dir = output_dir.join(&plan_dir_name);
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
