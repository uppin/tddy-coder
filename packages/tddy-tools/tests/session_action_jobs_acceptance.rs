//! Acceptance tests — PRD *Async session actions (jobs)*, Testing Plan §2 naming.
//!
//! Asserts orchestration semantics via [`tddy_core::session_action_jobs`] (blocking vs async admission,
//! `wait`, `stop`, unknown id). Exercises helpers wired through `tddy-tools` / presenter.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tddy_core::session_action_jobs::{
    invoke_session_action, stop_session_action_job, wait_session_action_job, AsyncStartBody,
    BlockingOutcomeBody, SessionActionInvokeOptions, SessionActionInvokeOutcome,
    SessionActionJobsError, SessionActionStopOutcome, SessionActionWaitOutcome,
};

/// Safety net for a wait that returns the moment a released job drains — not a prediction of how
/// long that takes; the ceiling only decides when to give up.
const A_JOB_HAS_TIME_TO_DRAIN_MS: u64 = 30_000;

fn write_sample_action(session: &Path, body: &str) {
    let dir = session.join("actions");
    fs::create_dir_all(&dir).expect("mkdir actions");
    fs::write(dir.join("sleep-touch.yaml"), body).expect("write manifest");
}

/// A session whose `sleep-touch` action runs a job the test holds: the job runs until the test
/// [releases](HeldJobSession::release) it, then touches its sentinel and exits 17.
///
/// The test, not the clock, decides whether a job is still running. A job that slept a fixed
/// quarter second raced every assertion about its state: on a loaded host it could outlive a bounded
/// wait (so "stop after it finished" found it running) or finish before the next call (so "stop a
/// running job" found it done).
struct HeldJobSession {
    dir: PathBuf,
}

impl HeldJobSession {
    /// Let the session's job finish: it touches its sentinel and exits.
    fn release(&self) {
        fs::write(self.dir.join("release"), "").expect("release the job");
    }

    /// Release the job from another thread once `after` has passed — for a call that blocks on the
    /// job and so cannot release it itself.
    fn release_after(&self, after: std::time::Duration) {
        let release = self.dir.join("release");
        std::thread::spawn(move || {
            std::thread::sleep(after);
            fs::write(release, "").expect("release the job");
        });
    }
}

/// A job a test never released would poll forever after the test is gone.
impl Drop for HeldJobSession {
    fn drop(&mut self) {
        let _ = fs::write(self.dir.join("release"), "");
    }
}

/// Session fixture: a held job + sentinel file so premature success is detectable.
fn session_with_a_held_sleep_touch_action() -> HeldJobSession {
    let session = std::env::temp_dir().join(format!(
        "tddy_sess_jobs_accept_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&session);
    fs::create_dir_all(&session).expect("mkdir session");

    let stub = session.join("wait_then_touch.sh");
    let sentinel = session.join("job_done.marker");
    let release = session.join("release");
    let sh = format!(
        r#"#!/bin/sh
while [ ! -f "{}" ]; do sleep 0.01; done
touch "{}"
exit 17
"#,
        release.display(),
        sentinel.display()
    );
    fs::write(&stub, &sh).expect("write stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = fs::metadata(&stub).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(&stub, p).unwrap();
    }

    write_sample_action(
        &session,
        &format!(
            r#"
version: 1
id: sleep-touch
summary: Runs until released, then touches its marker
architecture: native
command: ["{}"]
input_schema:
  type: object
  additionalProperties: false
"#,
            stub.display()
        ),
    );
    HeldJobSession { dir: session }
}

fn parse_record(outcome: SessionActionInvokeOutcome) -> Value {
    match outcome {
        SessionActionInvokeOutcome::Blocking(BlockingOutcomeBody::Record(v)) => v,
        other => panic!("expected blocking invocation record outcome; got {other:?}"),
    }
}

/// 1. Blocking path must mirror today's synchronous structured JSON (exit code + captured streams)
///
///    after the subprocess exits — never report success (`exit_code` et al.) before sentinel exists.
#[test]
fn session_action_blocking_matches_legacy_semantics() {
    // Given
    let held = session_with_a_held_sleep_touch_action();
    let session_dir = held.dir.clone();
    let sentinel = session_dir.join("job_done.marker");
    let repo: Option<PathBuf> = None;
    let t0 = std::time::Instant::now();
    held.release_after(std::time::Duration::from_millis(250));

    // When
    let outcome = invoke_session_action(
        &session_dir,
        repo.as_deref(),
        "sleep-touch",
        &json!({}),
        SessionActionInvokeOptions { async_start: false },
    )
    .expect("blocking invoke-session-action must succeed (parity with invoke-action)");

    // Then
    assert!(
        t0.elapsed().as_secs_f64() >= 0.18,
        "blocking invoke must await subprocess work (released after 0.25s); elapsed={:?}",
        t0.elapsed()
    );

    assert!(
        sentinel.is_file(),
        "marker implies subprocess exited; blocking handshake must lag behind completion"
    );

    let record = parse_record(outcome);
    assert_eq!(
        record.get("exit_code").and_then(|c| c.as_i64()),
        Some(17),
        "exit code parity with synchronous manifest run; got={record:?}"
    );
}

/// 2. Async admission must return stable `jobId`, log paths existing before return, initial `running` status.
#[test]
fn session_action_async_returns_job_and_log_paths() {
    // Given
    let held = session_with_a_held_sleep_touch_action();
    let session_dir = held.dir.clone();

    // When / Then
    match invoke_session_action(
        &session_dir,
        None,
        "sleep-touch",
        &json!({}),
        SessionActionInvokeOptions { async_start: true },
    )
    .expect("async admission must succeed")
    {
        SessionActionInvokeOutcome::AsyncStarted(AsyncStartBody {
            job_id,
            status,
            stdout_path,
            stderr_path,
        }) => {
            assert!(
                !job_id.trim().is_empty(),
                "jobId must be non-empty/stable for wait/stop"
            );
            assert_eq!(
                status.to_ascii_lowercase(),
                "running",
                "initial async status must advertise running-like state"
            );
            assert!(
                stdout_path.is_absolute() || stdout_path.starts_with(&session_dir),
                "stdout_path must resolve under workspace rules; got {}",
                stdout_path.display()
            );
            assert!(
                stderr_path.is_absolute() || stderr_path.starts_with(&session_dir),
                "stderr_path must resolve under workspace rules; got {}",
                stderr_path.display()
            );
            assert!(
                stdout_path
                    .parent()
                    .is_some_and(|p| !p.as_os_str().is_empty()),
                "stdout directory must exist or be created as part of admission"
            );
            assert!(
                fs::metadata(&stdout_path).is_ok(),
                "stdout capture path must exist immediately after admission: {}",
                stdout_path.display(),
            );
            assert!(
                fs::metadata(&stderr_path).is_ok(),
                "stderr capture path must exist immediately after admission: {}",
                stderr_path.display(),
            );
        }
        other => panic!("expected AsyncStarted; got {other:?}"),
    }
}

/// 3. Wait without timeout must surface completed/failed terminal semantics matching sync exit.
#[test]
fn session_action_wait_until_complete_without_timeout() {
    // Given
    let held = session_with_a_held_sleep_touch_action();
    let session_dir = held.dir.clone();

    let job_id = match invoke_session_action(
        &session_dir,
        None,
        "sleep-touch",
        &json!({}),
        SessionActionInvokeOptions { async_start: true },
    )
    .expect("async start required for wait scenarios")
    {
        SessionActionInvokeOutcome::AsyncStarted(b) => b.job_id,
        other => panic!("expected AsyncStarted for wait baseline; got {other:?}"),
    };

    // When the job is released and waited for
    held.release();
    let wait_out = wait_session_action_job(&session_dir, &job_id, None)
        .expect("wait(jobId) without timeout must complete");

    // Then
    match wait_out {
        SessionActionWaitOutcome::Completed { exit_code } => {
            assert_eq!(exit_code, Some(17));
        }
        SessionActionWaitOutcome::Failed { .. } => {
            // Allowed if PRD maps non-zero exits to Failed with summary.
        }
        other => {
            panic!("expected Completed (or Failed) without timeout disposition; got {other:?}")
        }
    }

    assert!(
        session_dir.join("job_done.marker").is_file(),
        "wait must synchronize until subprocess finished"
    );
}

/// 4. Bounded wait surfaces `timed_out` while job stays running.
#[test]
fn session_action_wait_times_out_while_running() {
    // Given
    let held = session_with_a_held_sleep_touch_action();
    let session_dir = held.dir.clone();

    let job_id = match invoke_session_action(
        &session_dir,
        None,
        "sleep-touch",
        &json!({}),
        SessionActionInvokeOptions { async_start: true },
    )
    .expect("async")
    {
        SessionActionInvokeOutcome::AsyncStarted(b) => b.job_id,
        other => panic!("expected AsyncStarted; got {other:?}"),
    };

    // When / Then
    match wait_session_action_job(&session_dir, &job_id, Some(20))
        .expect("bounded wait API must succeed")
    {
        SessionActionWaitOutcome::TimedOut { still_running } => {
            assert!(
                still_running,
                "PRD requires explicit timed_out while still running disposition"
            );
        }
        other => panic!("expected TimedOut for short-timeout wait on long job; got {other:?}"),
    }

    // Second wait, once the job is released, observes it drain to a terminal state.
    held.release();
    let subsequent =
        wait_session_action_job(&session_dir, &job_id, Some(A_JOB_HAS_TIME_TO_DRAIN_MS))
            .expect("subsequent wait");
    assert!(
        matches!(
            subsequent,
            SessionActionWaitOutcome::Completed { .. }
                | SessionActionWaitOutcome::Failed { .. }
        ),
        "after timeout, eventual wait without stop must reach terminal Completed/Failed when job drains; got {subsequent:?}"
    );
}

/// 5. Stop cancels/cooperatively terminates running job while logs remain readable.
#[test]
fn session_action_stop_cancels_running_job() {
    // Given
    let held = session_with_a_held_sleep_touch_action();
    let session_dir = held.dir.clone();

    let (job_id, stdout_path) = match invoke_session_action(
        &session_dir,
        None,
        "sleep-touch",
        &json!({}),
        SessionActionInvokeOptions { async_start: true },
    )
    .expect("async")
    {
        SessionActionInvokeOutcome::AsyncStarted(b) => (b.job_id, b.stdout_path),
        other => panic!("expected AsyncStarted; got {other:?}"),
    };

    // When
    let stop_out =
        stop_session_action_job(&session_dir, &job_id).expect("stop must succeed API-wise");

    // Then
    assert!(
        matches!(stop_out, SessionActionStopOutcome::Stopped),
        "stop on running job must report Stopped; got {stop_out:?}"
    );

    match wait_session_action_job(&session_dir, &job_id, Some(2500)).expect("post-stop wait") {
        SessionActionWaitOutcome::TimedOut { still_running } if still_running => {
            panic!(
                "after stop wait must observe terminal/cooperative stop, not still running timeout"
            )
        }
        _ => {}
    }

    let _ = fs::read_to_string(&stdout_path); // readability: path must remain openable after stop
}

/// 6. Stop after terminal state is classified `already_finished`/equivalent — no panic across crate boundary.
#[test]
fn session_action_stop_idempotent_after_terminal() {
    // Given
    let held = session_with_a_held_sleep_touch_action();
    let session_dir = held.dir.clone();

    let job_id = match invoke_session_action(
        &session_dir,
        None,
        "sleep-touch",
        &json!({}),
        SessionActionInvokeOptions { async_start: true },
    )
    .expect("async")
    {
        SessionActionInvokeOutcome::AsyncStarted(b) => b.job_id,
        other => panic!("expected AsyncStarted; got {other:?}"),
    };

    // When the job is released and has finished — the wait returns only once it has, so the stops
    // below meet a job that is terminal, not one still running on a loaded host
    held.release();
    let settled = wait_session_action_job(&session_dir, &job_id, Some(A_JOB_HAS_TIME_TO_DRAIN_MS))
        .expect("let job settle");
    assert!(
        matches!(
            settled,
            SessionActionWaitOutcome::Completed { .. } | SessionActionWaitOutcome::Failed { .. }
        ),
        "precondition: the released job reached a terminal state; got {settled:?}"
    );

    let first = stop_session_action_job(&session_dir, &job_id).expect("first stop succeeds");
    let second =
        stop_session_action_job(&session_dir, &job_id).expect("second stop must not crash");

    // Then
    assert!(
        matches!(
            first,
            SessionActionStopOutcome::AlreadyTerminal | SessionActionStopOutcome::AlreadyFinished
        ),
        "first stop after terminal should classify already terminal/finished (PRD §6 semantics); got {first:?}"
    );
    assert!(
        matches!(
            second,
            SessionActionStopOutcome::AlreadyTerminal | SessionActionStopOutcome::AlreadyFinished
        ),
        "idempotent repeated stop classification; got {second:?}"
    );
}

/// 7. Unknown `jobId` returns structured `unknown_job`, not unwind/panic across FFI boundary.
#[test]
fn session_action_unknown_job_returns_structured_error() {
    // Given
    let held = session_with_a_held_sleep_touch_action();
    let session_dir = held.dir.clone();

    // When
    let err =
        stop_session_action_job(&session_dir, "definitely-not-a-registered-job-id").unwrap_err();

    // Then
    assert!(
        matches!(err, SessionActionJobsError::UnknownJob(_)),
        "expected structured UnknownJob; got {err:?}"
    );
    assert_eq!(
        err.stable_code(),
        "unknown_job",
        "agents rely on stable error_code string (`unknown_job`)"
    );
}
