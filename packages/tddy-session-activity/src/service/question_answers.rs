//! `AwaitQuestionAnswer`: an orchestrated claude-cli session's `PermissionRequest` hook, held until
//! its orchestrator answers or the deadline passes (`#home-inbox` 6/8).
//!
//! Authenticated as `ReportSessionStatus` is: the session must be claude-cli and the `hook_token`
//! its own. Then, in order:
//!
//! 1. no `orchestrator_session_id` in the session's changeset, an orchestrator on another host, or
//!    one whose terminal this daemon does not serve → `NO_ORCHESTRATOR`, at once;
//! 2. the pending question must be the `request_id` asked for, and a permission;
//! 3. a waiter is opened in [`PendingAnswers`] and held for at most [`AnswerWaitPorts::wait`] →
//!    `ANSWERED`, or `TIMED_OUT`.

use std::sync::Arc;
use std::time::Duration;

use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::{AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse};

use crate::orchestrator_presence::{NoOrchestratorIsActive, OrchestratorPresence};
use crate::pending_answers::PendingAnswers;
use crate::service::ActivityServiceImpl;

/// What `AwaitQuestionAnswer` needs from the daemon: the registry the answer tool delivers into,
/// whether an orchestrator is present, and how long a hook may be held.
#[derive(Clone)]
pub struct AnswerWaitPorts {
    /// Shared with `AnswerSessionPermission`; one per daemon.
    pub pending: Arc<PendingAnswers>,
    pub presence: Arc<dyn OrchestratorPresence>,
    /// `claude_cli.answer_wait_secs`. Injected so a test can hold a hook for milliseconds.
    pub wait: Duration,
}

impl Default for AnswerWaitPorts {
    /// No orchestrator is ever present, so no hook is ever held: a host that wires nothing behaves
    /// as it did before `AwaitQuestionAnswer` existed.
    fn default() -> Self {
        Self {
            pending: Arc::new(PendingAnswers::new()),
            presence: Arc::new(NoOrchestratorIsActive),
            wait: Duration::from_secs(tddy_daemon_kernel::config::DEFAULT_ANSWER_WAIT_SECS),
        }
    }
}

impl ActivityServiceImpl {
    pub(super) async fn await_question_answer_impl(
        &self,
        req: AwaitQuestionAnswerRequest,
    ) -> Result<Response<AwaitQuestionAnswerResponse>, Status> {
        // TODO(permission-answer): implement — authenticate as `report_session_status_impl` does,
        // then the three steps of the module doc.
        let _ = (&self.answers, req);
        Err(Status::unimplemented(
            "AwaitQuestionAnswer is not implemented yet",
        ))
    }
}
