//! The permission prompts claude-cli hooks are holding open on this daemon, waiting for an
//! orchestrator's answer (`#home-inbox` 6/8).
//!
//! A child's `PermissionRequest` hook reports its question, then calls `AwaitQuestionAnswer`, which
//! [`PendingAnswers::open`]s a waiter under the question's `request_id` and
//! [`PendingAnswers::wait`]s on it. `AnswerSessionPermission` delivers the orchestrator's answer
//! here, through [`PermissionAnswering`] — `#coagents`' answering vocabulary, so a claude-cli child
//! and an ACP subagent refuse a second answer the same way.
//!
//! In memory, one waiter per `request_id`: a daemon restart drops every waiter, and each hook falls
//! back to Claude's own prompt. `ParkedPermissions` is not reused: it parks an ACP turn in process,
//! where this holds a hook process across an RPC.
//!
//! Feature doc: docs/ft/daemon/claude-cli-session.md.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tddy_discovery::subagent::{
    AnsweredBy, PermissionAnswer, PermissionAnswering, PermissionRequestView,
};

/// An answer as it reached a waiting hook: the answer, who gave it, and the message a `reject_*`
/// option carries for Claude.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveredAnswer {
    pub answer: PermissionAnswer,
    pub by: AnsweredBy,
    /// Why, for Claude, when the answer rejects; `None` otherwise.
    pub message: Option<String>,
}

/// How a [`PendingAnswers::wait`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitOutcome {
    Answered(DeliveredAnswer),
    /// Nobody answered before the deadline. The waiter is gone: a later answer is refused.
    TimedOut,
}

/// One open waiter: the session whose hook holds it and the request as an orchestrator reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenRequest {
    pub session_id: String,
    pub request: PermissionRequestView,
}

/// What [`PendingAnswers`] holds, behind one lock.
// TODO(permission-answer): read by the implementation; the stubs never do.
#[allow(dead_code)]
#[derive(Default)]
struct Slots {
    /// Requests a hook is waiting on, by `request_id`.
    open: HashMap<String, OpenRequest>,
    /// Requests already answered, by `request_id`, with who answered first. Kept after the waiter
    /// is gone, so a second answer is refused naming the first answerer.
    answered: HashMap<String, AnsweredBy>,
}

/// Every permission prompt a hook holds open on this daemon. Shared by `AwaitQuestionAnswer` and
/// `AnswerSessionPermission`; one instance per daemon.
#[derive(Default)]
pub struct PendingAnswers {
    slots: Mutex<Slots>,
}

impl PendingAnswers {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Open a waiter for `request`, asked by `session_id`'s hook. Re-opening a request id replaces
    /// the earlier waiter.
    pub fn open(&self, session_id: &str, request: PermissionRequestView) {
        // TODO(permission-answer): implement — insert into `open` under `request.request_id`.
        let _ = (&self.slots, session_id, request);
    }

    /// The request a hook holds open under `request_id`, if any.
    #[must_use]
    pub fn open_request(&self, request_id: &str) -> Option<OpenRequest> {
        // TODO(permission-answer): implement — look `request_id` up in `open`.
        let _ = (&self.slots, request_id);
        None
    }

    /// Wait until the waiter `request_id` names is answered, or `deadline` passes. A request no
    /// waiter is open for times out at once.
    pub async fn wait(&self, request_id: &str, deadline: Duration) -> WaitOutcome {
        // TODO(permission-answer): implement — a oneshot per waiter, raced against `deadline`;
        // on timeout the waiter is removed.
        let _ = (&self.slots, request_id, deadline);
        WaitOutcome::TimedOut
    }

    /// Deliver `answer` to its waiter with `message`, recording `by` as who answered. The first
    /// answer wins: a request already answered is refused as `permission request '<id>' was already
    /// answered by <by>`, and a request no hook is waiting on as `no hook is waiting on permission
    /// request '<id>'`. An answer delivered before the hook calls [`Self::wait`] is that wait's
    /// outcome.
    pub fn answer_with_message(
        &self,
        answer: &PermissionAnswer,
        by: AnsweredBy,
        message: Option<String>,
    ) -> Result<(), String> {
        // TODO(permission-answer): implement — refuse when `answered` names the request, then take
        // the waiter out of `open`, record `by` and wake it.
        let _ = (&self.slots, by, message);
        Err(format!(
            "permission request '{}': answering is not implemented",
            answer.request_id()
        ))
    }
}

impl PermissionAnswering for PendingAnswers {
    fn answer_as(&self, answer: &PermissionAnswer, by: AnsweredBy) -> Result<(), String> {
        self.answer_with_message(answer, by, None)
    }

    fn answered_by(&self, request_id: &str) -> Option<AnsweredBy> {
        // TODO(permission-answer): implement — look `request_id` up in `answered`.
        let _ = (&self.slots, request_id);
        None
    }

    /// Never: a claude-cli child's request is the orchestrator's to answer, and a person answers it
    /// in Claude's own terminal, not through this registry.
    fn deferred_to_user(&self, _request_id: &str) -> bool {
        false
    }

    /// The earliest-opened request a hook is still waiting on, across this daemon's sessions.
    fn pending_request(&self) -> Option<PermissionRequestView> {
        // TODO(permission-answer): implement — the oldest entry of `open`.
        let _ = &self.slots;
        None
    }
}
