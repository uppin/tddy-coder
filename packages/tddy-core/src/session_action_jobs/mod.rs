//! Async session actions (jobs): optional non-blocking start, wait, stop.
//!
//! PRD: async `invoke` returns `jobId` and log paths; `wait(jobId, timeout)`; `stop(jobId)`.
//! Jobs persist metadata under `<session_dir>/session_action_jobs/`.

mod error;
mod runner;

pub use error::SessionActionJobsError;

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Start mode for `invoke_session_action` (PRD: default blocking).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionActionInvokeOptions {
    /// When false, block until terminal state before returning outcome (legacy semantics).
    /// When true, admit the job and return immediately with identifiers and paths.
    pub async_start: bool,
}

/// Outcome when `async_start == false`: same structured record as synchronous `invoke-action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockingOutcomeBody {
    /// Mirror of `exit_code`, `stdout`, `stderr` plus optional summaries.
    Record(Value),
}

/// Outcome when `async_start == true`: immediate admission payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncStartBody {
    pub job_id: String,
    pub status: String,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionActionInvokeOutcome {
    Blocking(BlockingOutcomeBody),
    AsyncStarted(AsyncStartBody),
}

/// Result of `wait_session_action_job`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionActionWaitOutcome {
    Completed { exit_code: Option<i32> },
    Failed { exit_code: Option<i32>, error_summary: Option<String> },
    /// Positive timeout elapsed while job still running (PRD: agent may grep logs / extend wait).
    TimedOut { still_running: bool },
}

/// Result of `stop_session_action_job`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionActionStopOutcome {
    Stopped,
    AlreadyTerminal,
    /// Structured idempotency: stop after stop or after natural completion.
    AlreadyFinished,
}

/// Invoke a session action with optional async admission (PRD § functional req 1).
///
/// When `options.async_start` is false, behavior must match today's blocking `invoke-action` JSON.
/// When true, response must be immediate with `jobId`, paths, and initial `running` status.
pub fn invoke_session_action(
    session_dir: &Path,
    repo_root: Option<&Path>,
    action_id: &str,
    args: &Value,
    options: SessionActionInvokeOptions,
) -> Result<SessionActionInvokeOutcome, SessionActionJobsError> {
    runner::invoke_session_action(session_dir, repo_root, action_id, args, options)
}

/// Block until the job reaches a terminal state, or until `timeout_ms` elapses (PRD §2).
///
/// `timeout_ms`: `None` or `Some(0)` → unbounded; `Some(n)` for `n > 0` → bounded wait.
pub fn wait_session_action_job(
    session_dir: &Path,
    job_id: &str,
    timeout_ms: Option<u64>,
) -> Result<SessionActionWaitOutcome, SessionActionJobsError> {
    runner::wait_session_action_job(session_dir, job_id, timeout_ms)
}

/// Request cooperative cancellation / stop (PRD §3).
pub fn stop_session_action_job(
    session_dir: &Path,
    job_id: &str,
) -> Result<SessionActionStopOutcome, SessionActionJobsError> {
    runner::stop_session_action_job(session_dir, job_id)
}

/// Handle tied to `<session_dir>/session_action_jobs` layout (PRD job table root).
#[derive(Debug, Clone)]
pub struct SessionActionJobRegistry {
    _session_dir: PathBuf,
}

impl SessionActionJobRegistry {
    /// Prepares `session_action_jobs/` and `jobs/` under the session directory.
    pub fn load(session_dir: &Path) -> Result<Self, SessionActionJobsError> {
        runner::ensure_jobs_layout(session_dir)?;
        Ok(Self {
            _session_dir: session_dir.to_path_buf(),
        })
    }

    #[allow(dead_code)]
    pub fn session_dir(&self) -> &Path {
        self._session_dir.as_path()
    }
}
