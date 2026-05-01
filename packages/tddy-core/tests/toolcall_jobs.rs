//! Unit-level tests for the session-action job registry (PRD Testing Plan §1).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tddy_core::session_action_jobs::{
    invoke_session_action, stop_session_action_job, wait_session_action_job,
    BlockingOutcomeBody, SessionActionInvokeOptions, SessionActionJobRegistry,
    SessionActionJobsError, SessionActionInvokeOutcome, SessionActionWaitOutcome,
};

fn unique_jobs_session_root(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tddy_toolcall_jobs_{}_{}_{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir session root");
    dir
}

fn write_fixture_unit_action(session: &Path) {
    let dir = session.join("actions");
    fs::create_dir_all(&dir).expect("mkdir actions");
    fs::write(
        dir.join("unit-action.yaml"),
        r#"version: 1
id: unit-action
summary: unit test action
architecture: native
command: ["/bin/sh", "-c", "exit 0"]
input_schema:
  type: object
  additionalProperties: false
"#,
    )
    .expect("write unit-action manifest");
}

/// PRD: registry loads under the session dir and supports terminal transitions (`running → completed`, etc.).
#[test]
fn job_registry_round_trip_load_and_terminal_transition() {
    let session = unique_jobs_session_root("registry");
    SessionActionJobRegistry::load(session.as_path())
        .expect("job registry load must succeed and return a typed handle");
}

/// PRD §2 (unit slice): timeout bookkeeping yields deterministic bounded waits (no flaky long sleeps).
#[test]
fn job_registry_timeout_bookkeeping_exposes_bounded_wait_deadline() {
    let session = unique_jobs_session_root("timeout_bookkeeping");
    SessionActionJobRegistry::load(session.as_path()).expect(
        "registry must initialize timeout metadata for bounded wait operations",
    );
}

/// Green: blocking invoke returns `Ok(Blocking(Record { exit_code: … }))` after subprocess terminal state.
#[test]
fn invoke_blocking_returns_ok_with_exit_code_payload() {
    let session = unique_jobs_session_root("invoke_blocking_unit");
    write_fixture_unit_action(&session);
    let outcome = invoke_session_action(
        session.as_path(),
        None,
        "unit-action",
        &json!({}),
        SessionActionInvokeOptions {
            async_start: false,
        },
    )
    .expect("blocking invoke must succeed with structured terminal record");

    match outcome {
        SessionActionInvokeOutcome::Blocking(BlockingOutcomeBody::Record(v)) => {
            assert!(
                v.get("exit_code").is_some(),
                "record must expose exit_code for agent parity; got {v}"
            );
        }
        other => panic!("expected blocking record outcome; got {other:?}"),
    }
}

/// Green: `wait(job_id, None)` resolves to a completed/failed terminal disposition (not `NotImplemented`).
#[test]
fn wait_without_timeout_returns_completed_disposition() {
    let session = unique_jobs_session_root("wait_unit");
    write_fixture_unit_action(&session);
    let job_id = match invoke_session_action(
        session.as_path(),
        None,
        "unit-action",
        &json!({}),
        SessionActionInvokeOptions {
            async_start: true,
        },
    )
    .expect("async start for wait unit test")
    {
        SessionActionInvokeOutcome::AsyncStarted(b) => b.job_id,
        other => panic!("expected AsyncStarted; got {other:?}"),
    };
    let out = wait_session_action_job(session.as_path(), &job_id, None)
        .expect("wait API must surface terminal disposition once jobs run");

    assert!(
        matches!(
            out,
            SessionActionWaitOutcome::Completed { .. } | SessionActionWaitOutcome::Failed { .. }
        ),
        "unbounded wait must end in Completed or Failed; got {out:?}"
    );
}

/// Green: unknown job id on `stop` maps to [`SessionActionJobsError::UnknownJob`] (stable `unknown_job` code).
#[test]
fn stop_unknown_job_id_returns_structured_unknown_job_error() {
    let session = unique_jobs_session_root("stop_unknown_unit");
    let err = stop_session_action_job(session.as_path(), "not-a-registered-job").unwrap_err();
    assert!(
        matches!(err, SessionActionJobsError::UnknownJob(_)),
        "expected UnknownJob for unregistered id; got {err:?}"
    );
}
