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

use std::path::Path;

use tddy_core::pending_question::read_pending_question;
use tddy_core::read_changeset;
use tddy_discovery::subagent::PermissionAnswer;
use tddy_rpc::{Response, Status};
use tddy_service::proto::activity::{
    AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse, AwaitQuestionOutcome,
};

use crate::answers::permission_request_view;
use crate::pending_answers::{DeliveredAnswer, WaitOutcome};

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
        let (_sessions_base, session_dir) = self.hook_session_dir(&req.os_user, &req.session_id)?;
        let meta = self.hook_session(&session_dir)?;
        if meta.session_type.as_deref() != Some("claude-cli") {
            return Err(Status::failed_precondition(
                "session_type is not claude-cli",
            ));
        }
        if meta.hook_token.as_deref().unwrap_or("") != req.hook_token {
            return Err(Status::permission_denied("invalid hook_token"));
        }

        // 1. Somebody on this host must be able to answer, or the hook is told so at once.
        if !self.orchestrator_is_present(&session_dir).await {
            return Ok(Response::new(outcome_only(
                AwaitQuestionOutcome::NoOrchestrator,
            )));
        }

        // 2. The question is the one asked for, and a permission.
        let question = read_pending_question(&session_dir)
            .map_err(|e| Status::internal(format!("cannot read the pending question: {e}")))?
            .filter(|question| !req.request_id.is_empty() && question.request_id == req.request_id)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "no pending question '{}' on session {}",
                    req.request_id, req.session_id
                ))
            })?;
        if question.kind != "permission" {
            return Err(Status::failed_precondition(format!(
                "pending question '{}' is a {} question, not a permission",
                req.request_id, question.kind
            )));
        }

        // 3. Hold the hook until it is answered or the deadline passes.
        let has_suggestions = has_permission_suggestions(&req.permission_suggestions_json);
        let pending = &self.answers.pending;
        pending.open(
            &req.session_id,
            permission_request_view(&question, has_suggestions),
        );
        let response = match pending.wait(&req.request_id, self.answers.wait).await {
            WaitOutcome::Answered(delivered) => answered_response(delivered),
            WaitOutcome::TimedOut => outcome_only(AwaitQuestionOutcome::TimedOut),
        };
        Ok(Response::new(response))
    }

    /// Whether the session's orchestrator is on this host and served here.
    async fn orchestrator_is_present(&self, session_dir: &Path) -> bool {
        let changeset = match read_changeset(session_dir) {
            Ok(changeset) => changeset,
            Err(e) => {
                log::debug!(
                    target: "tddy_session_activity::question_answers",
                    "no orchestrator for {}: {e}",
                    session_dir.display()
                );
                return false;
            }
        };
        let Some(orchestrator) = changeset
            .orchestrator_session_id
            .as_deref()
            .filter(|id| !id.is_empty())
        else {
            return false;
        };
        let on_another_host = changeset
            .orchestrator_daemon_instance_id
            .as_deref()
            .is_some_and(|host| !host.is_empty() && host != self.ports.daemon_instance_id);
        !on_another_host && self.answers.presence.is_active(orchestrator).await
    }
}

/// A response that only says how the wait ended.
fn outcome_only(outcome: AwaitQuestionOutcome) -> AwaitQuestionAnswerResponse {
    AwaitQuestionAnswerResponse {
        outcome: outcome as i32,
        ..Default::default()
    }
}

fn answered_response(delivered: DeliveredAnswer) -> AwaitQuestionAnswerResponse {
    let (option_id, cancelled) = match delivered.answer {
        PermissionAnswer::Selected { option_id, .. } => (option_id, false),
        PermissionAnswer::Cancelled { .. } => (String::new(), true),
    };
    AwaitQuestionAnswerResponse {
        outcome: AwaitQuestionOutcome::Answered as i32,
        option_id,
        cancelled,
        message: delivered.message.unwrap_or_default(),
        answered_by: delivered.by.name(),
        // A permission answer carries no per-question answers (`#home-inbox` 8/8 fills them).
        answers: Vec::new(),
    }
}

/// Whether Claude's `permission_suggestions` hold at least one entry: only then is "allow always"
/// offered.
fn has_permission_suggestions(permission_suggestions_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(permission_suggestions_json)
        .ok()
        .and_then(|suggestions| suggestions.as_array().map(|entries| !entries.is_empty()))
        .unwrap_or(false)
}
