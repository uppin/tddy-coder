//! When a plan goal fails (e.g. output parsing error), debug.log should still be
//! written to the session dir/logs/ so the developer can diagnose the failure.
//! Lives here (not tddy-core unit tests) so we can use `TddRecipe` without a circular dev-dep.

use std::sync::Arc;

use serial_test::serial;
use tddy_core::presenter::workflow_runner::run_workflow;
use tddy_core::presenter::WorkflowEvent;
use tddy_core::{MockBackend, SharedBackend, WorkflowRecipe};
use tddy_workflow_recipes::TddRecipe;

/// Reproduces: resolve_log_defaults is only called on the success path in
/// run_plan_without_output_dir; on error the function returns None before
/// reaching the resolve_log_defaults call.
#[test]
#[serial]
#[cfg(unix)]
fn debug_log_written_to_session_dir_when_plan_fails() {
    let tmp = std::env::temp_dir().join("tddy-debug-log-on-plan-fail");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let backend = Arc::new(MockBackend::new());
    backend.push_ok("not valid plan json");

    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let (_answer_tx, answer_rx) = std::sync::mpsc::channel();

    let session_id = "test-debug-log-session";

    tddy_core::init_tddy_logger_legacy(false, None, None);

    run_workflow(
        Arc::new(TddRecipe) as Arc<dyn WorkflowRecipe>,
        SharedBackend::from_arc(backend),
        event_tx,
        answer_rx,
        tmp.clone(),
        None,
        Some(session_id.to_string()),
        None,
        Some("Build test feature".to_string()),
        None,
        None,
        false,
        None,
        None,
    );

    let mut got_error = false;
    let mut error_msg = String::new();
    while let Ok(event) = event_rx.try_recv() {
        if let WorkflowEvent::WorkflowComplete(Err(ref msg)) = event {
            got_error = true;
            error_msg = msg.clone();
        }
    }
    assert!(got_error, "should get a workflow error from plan failure");

    let session_dir = tmp
        .join(tddy_core::output::SESSIONS_SUBDIR)
        .join(session_id);
    assert!(
        session_dir.exists(),
        "session dir should be created by PlanTask at {}",
        session_dir.display()
    );

    let debug_log = session_dir.join("logs").join("debug.log");
    assert!(
        debug_log.exists(),
        "debug.log should exist at {}/logs/ even when plan fails (error: {})",
        session_dir.display(),
        error_msg,
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
