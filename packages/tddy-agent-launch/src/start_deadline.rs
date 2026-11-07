//! The deadline a whole session start runs under (`session_start_timeout_secs`, 600 s by default).
//!
//! Every wait inside a start has a bound of its own — a supervisor call, a clone's progress, an ACP
//! `open_session` — but a start strings many of them together, and some steps (a worktree cut, an
//! index build) have none. [`StartDeadline`] bounds the whole: when it expires the start's
//! cancellation token is cancelled, exactly as a caller's cancel would, and the start is given
//! [`START_CANCEL_GRACE`] to stop at its next checkpoint. A start that comes back with a session
//! inside the grace has that session undone (the existing cancelled-start cleanup); one that does
//! not come back is dropped. Either way the caller gets `DEADLINE_EXCEEDED` naming the step
//! ([`StartStep`]) the start had begun and not ended, as its progress reported it.
//!
//! Feature docs: docs/ft/daemon/1-WIP/PRD-2026-11-06-bounded-session-waits.md

use std::future::Future;
use std::time::Duration;

use tddy_rpc::Status;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_task::CancellationToken;

/// How long a start whose deadline expired is given, once cancelled, to come back with what it
/// created so it can be undone. A start still running after this is dropped.
pub const START_CANCEL_GRACE: Duration = Duration::from_secs(30);

/// The bound one session start runs under.
#[derive(Debug, Clone, Copy)]
pub struct StartDeadline {
    deadline: Duration,
    cancel_grace: Duration,
}

impl StartDeadline {
    /// A start bounded by `deadline` (the daemon passes `DaemonConfig::session_start_timeout`),
    /// with [`START_CANCEL_GRACE`] to come back once cancelled.
    pub fn new(deadline: Duration) -> StartDeadline {
        StartDeadline {
            deadline,
            cancel_grace: START_CANCEL_GRACE,
        }
    }

    /// This deadline with `grace` instead of [`START_CANCEL_GRACE`].
    pub fn with_cancel_grace(mut self, grace: Duration) -> StartDeadline {
        self.cancel_grace = grace;
        self
    }

    /// Run `start` under this deadline.
    ///
    /// `start` is handed the sink it reports its progress to: every event reaches `progress`
    /// unchanged, and the phases it begins and ends are how the deadline knows which step was
    /// running — also when `progress` reports nowhere (the unary start). `cancel` is the start's own
    /// token, cancelled when the deadline expires. `undo` is run on a start that returned a session
    /// after its deadline expired, so the timed-out start leaves nothing behind.
    ///
    /// Returns `start`'s own result when it finished in time; otherwise `DEADLINE_EXCEEDED` naming
    /// the deadline and the running step.
    pub async fn run<T, S, SF, U, UF>(
        &self,
        progress: &AttachmentProgressSink,
        cancel: &CancellationToken,
        start: S,
        undo: U,
    ) -> Result<T, Status>
    where
        S: FnOnce(AttachmentProgressSink) -> SF,
        SF: Future<Output = Result<T, Status>>,
        U: FnOnce(T) -> UF,
        UF: Future<Output = ()>,
    {
        // TODO(bounded-waits): implement — track the begun-and-not-ended phase through a sink that
        // forwards to `progress`; on `self.deadline` cancel `cancel`, wait up to
        // `self.cancel_grace`, `undo` a session that came back, and fail `DEADLINE_EXCEEDED`
        // naming the deadline and that step.
        let _ = (self.deadline, self.cancel_grace, cancel, undo);
        start(progress.clone()).await
    }
}
