//! The delivery side of a followed hook log (split out of [`crate::remote_hook_follower`]): the
//! daemon's end of a delivered hook ([`HookReportDelivery`]) and the production [`HookSink`] that maps
//! each followed line onto it ([`ActivityHookSink`]). Re-exported from `remote_hook_follower`, so
//! the paths other packages name are unchanged.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_remote_shell::RemoteShell;

use crate::remote_hook_follower::{HookLine, HookSink};

/// The daemon end of a delivered hook: the two hook reports a local `tddy-tools session-hook`
/// makes, served in-process. Implemented on the session host, which owns the activity service;
/// a report the handling refuses is the delivery's error, never retried by the follower.
#[async_trait]
pub trait HookReportDelivery: Send + Sync {
    /// Serve one `ReportSessionStatus` request, as the activity service's RPC does, and return its
    /// response: a permission or a question is answered with the `request_id` it was recorded under.
    async fn report_session_status(
        &self,
        request: tddy_service::proto::activity::ReportSessionStatusRequest,
    ) -> Result<tddy_service::proto::activity::ReportSessionStatusResponse, String>;
    /// Hold for the orchestrator's answer to a recorded request, as the activity service's
    /// `AwaitQuestionAnswer` does for a local hook: the same presence check, the same recorded
    /// wait, the same outcomes.
    async fn await_question_answer(
        &self,
        request: tddy_service::proto::activity::AwaitQuestionAnswerRequest,
    ) -> Result<tddy_service::proto::activity::AwaitQuestionAnswerResponse, String>;
    /// Serve one `ReportAgentActivity` request, as the activity service's RPC does.
    async fn report_agent_activity(
        &self,
        request: tddy_service::proto::activity::ReportAgentActivityRequest,
    ) -> Result<(), String>;
    /// Post a target child's report to its orchestrator as a session message (`#orchestra` 2/9),
    /// as `PostSessionMessage` from the child would. A delivery that serves no session messages
    /// refuses it, naming why; the session host's delivery serves them.
    async fn post_message_to_orchestrator(
        &self,
        message: crate::back_channel::RemoteMessage,
    ) -> Result<(), String> {
        Err(format!(
            "this hook delivery posts no session messages; {}'s report was not delivered",
            message.session_id
        ))
    }
}

/// The production [`HookSink`]: each followed hook is mapped to the daemon's hook reports by
/// the one payload-to-request mapping (`tddy_session_activity::hook_request`, which the local
/// `tddy-tools session-hook` also reports with) and served through `delivery`. A refused report
/// is logged and the next hook still arrives: a hook must never block on the daemon's answer.
pub struct ActivityHookSink {
    delivery: Arc<dyn HookReportDelivery>,
    session_id: String,
    hook_token: String,
    os_user: String,
    /// Where a waiting hook's answer is written: the session's shell, its worktree on the target
    /// and its hooks' answer wait. `None` delivers report-only.
    answers: Option<(Arc<dyn RemoteShell>, std::path::PathBuf, u64)>,
}

impl ActivityHookSink {
    /// The sink of `session_id`'s hooks, authenticated with its `hook_token` as `os_user`.
    pub fn new(
        delivery: Arc<dyn HookReportDelivery>,
        session_id: &str,
        hook_token: &str,
        os_user: &str,
    ) -> Self {
        Self {
            delivery,
            session_id: session_id.to_string(),
            hook_token: hook_token.to_string(),
            os_user: os_user.to_string(),
            answers: None,
        }
    }

    /// This sink, answering the waiting hooks it delivers: once the orchestrator answers, Claude's
    /// decision is written to `<worktree>/<REMOTE_ANSWERS_DIR>/<nonce>` through `shell`, where the
    /// hook on the target waits for it (`#target-parity` 1/7). `answer_wait_secs` is the wait the
    /// session's hooks were rendered with: an answer still there once it has passed is removed.
    #[must_use]
    pub fn answering_through(
        mut self,
        shell: Arc<dyn RemoteShell>,
        worktree: &Path,
        answer_wait_secs: u64,
    ) -> Self {
        self.answers = Some((shell, worktree.to_path_buf(), answer_wait_secs));
        self
    }

    /// Hold for the answer of a waiting hook and write it, in a task of its own: the follower's
    /// reader must not stall on an orchestrator that takes minutes. A hook that only reports, or a
    /// sink that does not answer, holds nothing.
    fn answer_in_background(&self, line: &HookLine, request_id: Option<String>) {
        let (Some(nonce), Some((shell, worktree, answer_wait_secs))) =
            (&line.answer_nonce, &self.answers)
        else {
            return;
        };
        tokio::spawn(crate::remote_hook_answer::hold_and_answer(
            Arc::clone(&self.delivery),
            Arc::clone(shell),
            worktree.clone(),
            nonce.clone(),
            crate::remote_hook_answer::HeldHook {
                session_id: self.session_id.clone(),
                hook_token: self.hook_token.clone(),
                os_user: self.os_user.clone(),
                request_id,
                permission_suggestions: line.payload.get("permission_suggestions").cloned(),
                question_input: line.question_input(),
                window_ends: crate::remote_hook_answer::hook_window_end(*answer_wait_secs),
            },
        ));
    }
}

#[async_trait]
impl HookSink for ActivityHookSink {
    async fn deliver(&self, line: HookLine) {
        if line.event == tddy_core::REMOTE_MESSAGE_EVENT {
            crate::remote_hook_message::post_reported_message(
                self.delivery.as_ref(),
                &line,
                &self.session_id,
                &self.os_user,
            )
            .await;
            return;
        }
        let payload = line.payload.to_string();
        if let Some(request) = crate::remote_hook_report::reports_a_status(&line)
            .then(|| {
                tddy_session_activity::hook_request::session_status_request(
                    &self.session_id,
                    &self.hook_token,
                    &self.os_user,
                    &line.event,
                    &payload,
                )
            })
            .flatten()
        {
            let request_id = match self.delivery.report_session_status(request).await {
                Ok(response) => Some(response.request_id),
                Err(error) => {
                    log::warn!(
                        "remote hook {} for session {} refused: {error}",
                        line.event,
                        self.session_id
                    );
                    None
                }
            };
            self.answer_in_background(&line, request_id);
        }
        if let Some(request) = tddy_session_activity::hook_request::agent_activity_request(
            &line.event,
            &payload,
            &self.session_id,
            &self.hook_token,
            &self.os_user,
        ) {
            if let Err(error) = self.delivery.report_agent_activity(request).await {
                log::warn!(
                    "remote agent activity {} for session {} refused: {error}",
                    line.event,
                    self.session_id
                );
            }
        }
    }
}
