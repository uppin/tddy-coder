//! The delivery side of a followed hook log (split out of [`crate::remote_hook_follower`]): the
//! daemon's end of a delivered hook ([`HookReportDelivery`]) and the production [`HookSink`] that maps
//! each followed line onto it ([`ActivityHookSink`]). Re-exported from `remote_hook_follower`, so
//! the paths other packages name are unchanged.
//!
//! TODO(restructure-path-attr): moved here by hand, not by `tddy-tools restructure`. The engine
//! refused this `move_item` ("plan is malformed": its module-file resolution ignores the `#[path]`
//! test modules of `opencode_cli_spawn/restore_tabs.rs`), see
//! `docs/dev/todo/2026-11-21-restructure-ignores-path-attribute-modules.md`.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_remote_shell::RemoteShell;

use crate::remote_hook_follower::{HookLine, HookSink};

/// Why a followed report was refused: the activity service's `Status`, carried across the
/// in-process delivery so the follower can tell a report its session is not registered for yet
/// (held and retried, n1) from any other refusal (consumed). Defined here, not as `tddy_rpc::Status`
/// itself, so implementors outside this crate (the session driver, the tests) need no new
/// dependency to name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookDeliveryError {
    code: tddy_rpc::Code,
    message: String,
}

impl HookDeliveryError {
    /// The refusal of `status`, as the activity service answered it.
    pub fn from_status(status: tddy_rpc::Status) -> Self {
        Self {
            code: status.code(),
            message: status.message().to_string(),
        }
    }

    /// A refusal for a reason the activity service never gives (a delivery that serves no
    /// transcripts, no session messages): `FAILED_PRECONDITION`, never retried.
    pub fn failed_precondition(message: impl Into<String>) -> Self {
        Self {
            code: tddy_rpc::Code::FailedPrecondition,
            message: message.into(),
        }
    }

    /// The refusal of a report its session is not registered for yet, as `hook_session` answers
    /// it: `NOT_FOUND` with [`tddy_session_activity::SESSION_NOT_REGISTERED_YET`]. For the tests
    /// that hold a line until registration.
    pub fn session_not_registered_yet() -> Self {
        Self {
            code: tddy_rpc::Code::NotFound,
            message: tddy_session_activity::SESSION_NOT_REGISTERED_YET.to_string(),
        }
    }

    /// The RPC code of the refusal.
    pub fn code(&self) -> tddy_rpc::Code {
        self.code
    }

    /// The refusal's message, as the activity service worded it.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for HookDeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HookDeliveryError {}

/// The daemon end of a delivered hook: the two hook reports a local `tddy-tools session-hook`
/// makes, served in-process. Implemented on the session host, which owns the activity service.
/// A report the handling refuses is the delivery's error: the follower holds and retries one the
/// session is not registered for yet (n1), and consumes every other refusal.
#[async_trait]
pub trait HookReportDelivery: Send + Sync {
    /// Serve one `ReportSessionStatus` request, as the activity service's RPC does, and return its
    /// response: a permission or a question is answered with the `request_id` it was recorded under.
    async fn report_session_status(
        &self,
        request: tddy_service::proto::activity::ReportSessionStatusRequest,
    ) -> Result<tddy_service::proto::activity::ReportSessionStatusResponse, HookDeliveryError>;
    /// Hold for the orchestrator's answer to a recorded request, as the activity service's
    /// `AwaitQuestionAnswer` does for a local hook: the same presence check, the same recorded
    /// wait, the same outcomes.
    async fn await_question_answer(
        &self,
        request: tddy_service::proto::activity::AwaitQuestionAnswerRequest,
    ) -> Result<tddy_service::proto::activity::AwaitQuestionAnswerResponse, HookDeliveryError>;
    /// Serve one `ReportAgentActivity` request, as the activity service's RPC does.
    async fn report_agent_activity(
        &self,
        request: tddy_service::proto::activity::ReportAgentActivityRequest,
    ) -> Result<(), HookDeliveryError>;
    /// Record one line of a session's agent transcript, already mapped by its spec, as the activity
    /// service's `record_transcript_line` does — the session on this host or on a target alike. A
    /// delivery that records no transcripts refuses it, naming why; the session host's records them.
    async fn record_transcript_line(
        &self,
        line: tddy_session_activity::TranscriptLine,
    ) -> Result<(), HookDeliveryError> {
        Err(HookDeliveryError::failed_precondition(format!(
            "this hook delivery records no transcripts; a line of {}'s was not recorded",
            line.session_id
        )))
    }
    /// Post a target child's report to its orchestrator as a session message (`#orchestra` 2/9),
    /// as `PostSessionMessage` from the child would. A delivery that serves no session messages
    /// refuses it, naming why; the session host's delivery serves them.
    async fn post_message_to_orchestrator(
        &self,
        message: crate::back_channel::RemoteMessage,
    ) -> Result<(), HookDeliveryError> {
        Err(HookDeliveryError::failed_precondition(format!(
            "this hook delivery posts no session messages; {}'s report was not delivered",
            message.session_id
        )))
    }
}

/// Whether `error` refuses a report only because its session is not registered yet: the start
/// writes `.session.yaml` after the launch, and the session's followers start inside it. A refused
/// line with this error is held and retried until the session exists, never treated as consumed.
pub(crate) fn session_not_registered_yet(error: &HookDeliveryError) -> bool {
    error.code() == tddy_rpc::Code::NotFound
        && error.message() == tddy_session_activity::SESSION_NOT_REGISTERED_YET
}

/// How long a sink first waits before retrying a report its session is not registered for
/// yet; each retry doubles it, up to [`REGISTRATION_RETRY_MAX`]. The start writes
/// `.session.yaml` seconds after the follower starts, so the first hooks wait rather than drop.
const REGISTRATION_RETRY: std::time::Duration = std::time::Duration::from_millis(100);
const REGISTRATION_RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(1);

/// The production [`HookSink`]: each followed hook is mapped to the daemon's hook reports by
/// the one payload-to-request mapping (`tddy_session_activity::hook_request`, which the local
/// `tddy-tools session-hook` also reports with) and served through `delivery`. A report refused
/// only because its session is not registered yet is held and retried until the session exists
/// (n1); any other refusal is logged and the next hook still arrives.
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
                event: line.event.clone(),
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
            let request_id = match self
                .report_status_holding_for_registration(&line.event, request)
                .await
            {
                Ok(response) => Some(response.request_id),
                Err(error) => {
                    log::warn!(
                        "remote hook {} for session {} refused: {}",
                        line.event,
                        self.session_id,
                        error.message()
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
            if let Err(error) = self
                .report_activity_holding_for_registration(&line.event, request)
                .await
            {
                log::warn!(
                    "remote agent activity {} for session {} refused: {}",
                    line.event,
                    self.session_id,
                    error.message()
                );
            }
        }
    }
}

impl ActivityHookSink {
    /// Serve one status report, holding and retrying while its session is not registered yet.
    async fn report_status_holding_for_registration(
        &self,
        event: &str,
        request: tddy_service::proto::activity::ReportSessionStatusRequest,
    ) -> Result<tddy_service::proto::activity::ReportSessionStatusResponse, HookDeliveryError> {
        let mut wait = REGISTRATION_RETRY;
        let mut held = false;
        loop {
            match self.delivery.report_session_status(request.clone()).await {
                Err(status) if session_not_registered_yet(&status) => {
                    if !held {
                        held = true;
                        log::info!(
                            "remote hook {event} for session {} waits for registration",
                            self.session_id
                        );
                    }
                    tokio::time::sleep(wait).await;
                    wait = (wait * 2).min(REGISTRATION_RETRY_MAX);
                }
                outcome => return outcome,
            }
        }
    }

    /// Serve one activity report, holding and retrying while its session is not registered yet.
    async fn report_activity_holding_for_registration(
        &self,
        event: &str,
        request: tddy_service::proto::activity::ReportAgentActivityRequest,
    ) -> Result<(), HookDeliveryError> {
        let mut wait = REGISTRATION_RETRY;
        let mut held = false;
        loop {
            match self.delivery.report_agent_activity(request.clone()).await {
                Err(status) if session_not_registered_yet(&status) => {
                    if !held {
                        held = true;
                        log::info!(
                            "remote agent activity {event} for session {} waits for registration",
                            self.session_id
                        );
                    }
                    tokio::time::sleep(wait).await;
                    wait = (wait * 2).min(REGISTRATION_RETRY_MAX);
                }
                outcome => return outcome,
            }
        }
    }
}

/// Hold `record_transcript_line`'s delivery while its session is not registered yet, for the
/// transcript follower: the same wait the hook sink holds its reports with.
pub(crate) async fn record_transcript_holding_for_registration(
    delivery: &Arc<dyn HookReportDelivery>,
    line: tddy_session_activity::TranscriptLine,
) -> Result<(), HookDeliveryError> {
    let mut wait = REGISTRATION_RETRY;
    let mut held = false;
    loop {
        match delivery.record_transcript_line(line.clone()).await {
            Err(status) if session_not_registered_yet(&status) => {
                if !held {
                    held = true;
                    log::info!(
                        "agent transcript line of session {} waits for registration",
                        line.session_id
                    );
                }
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(REGISTRATION_RETRY_MAX);
            }
            outcome => return outcome,
        }
    }
}
