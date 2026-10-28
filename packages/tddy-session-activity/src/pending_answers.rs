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

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::oneshot;

use tddy_discovery::subagent::{
    AnsweredBy, PermissionAnswer, PermissionAnswering, PermissionRequestView,
};

mod question;

// `#home-inbox` 8/8: the waiters of `AskUserQuestion` calls, `PendingAnswers`' question methods.
pub use question::{DeliveredQuestionAnswers, OpenQuestion, QuestionWaitOutcome};

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

/// An open request and the order it was opened in.
struct Opened {
    seq: u64,
    open: OpenRequest,
}

/// What [`PendingAnswers`] holds, behind one lock.
#[derive(Default)]
struct Slots {
    /// Requests a hook is waiting on, by `request_id`.
    open: HashMap<String, Opened>,
    /// The wake-up of each request a hook is currently [`PendingAnswers::wait`]ing on.
    waiters: HashMap<String, oneshot::Sender<DeliveredAnswer>>,
    /// Answers delivered before their hook began waiting; the wait takes its own.
    delivered: HashMap<String, DeliveredAnswer>,
    /// Requests already answered, by `request_id`, with who answered first. Kept after the waiter
    /// is gone, so a second answer is refused naming the first answerer — for the last
    /// [`ANSWERED_REQUESTS_KEPT`] answers, oldest forgotten first (`answered_order`).
    answered: HashMap<String, AnsweredBy>,
    /// The keys of `answered`, oldest first.
    answered_order: VecDeque<String>,
    /// The next [`Opened::seq`].
    next_seq: u64,
}

/// How many answered requests [`PendingAnswers`] remembers, so a second answer is refused naming
/// the first answerer. The registry lives as long as the daemon, so the memory is bounded: an answer
/// to a request forgotten this way is refused as one no hook is waiting on.
pub const ANSWERED_REQUESTS_KEPT: usize = 1024;

impl Slots {
    /// Remember that `by` answered `request_id`, forgetting the oldest answer past
    /// [`ANSWERED_REQUESTS_KEPT`].
    fn remember_answer(&mut self, request_id: &str, by: AnsweredBy) {
        self.answered.insert(request_id.to_string(), by);
        self.answered_order.push_back(request_id.to_string());
        while self.answered_order.len() > ANSWERED_REQUESTS_KEPT {
            if let Some(oldest) = self.answered_order.pop_front() {
                self.answered.remove(&oldest);
            }
        }
    }

    /// Whether the open request under `request_id` is still the one opened as `seq`.
    fn is_open_as(&self, request_id: &str, seq: u64) -> bool {
        self.open
            .get(request_id)
            .is_some_and(|opened| opened.seq == seq)
    }
}

/// Every permission prompt a hook holds open on this daemon. Shared by `AwaitQuestionAnswer` and
/// `AnswerSessionPermission`; one instance per daemon.
#[derive(Default)]
pub struct PendingAnswers {
    slots: Mutex<Slots>,
    /// The `AskUserQuestion` waiters (`#home-inbox` 8/8); see [`question`].
    questions: Mutex<question::QuestionSlots>,
}

impl PendingAnswers {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The slots. A poisoned lock still holds consistent maps (every critical section is a few map
    /// operations), so it is recovered rather than turned into a panic in a hook's request.
    fn slots(&self) -> MutexGuard<'_, Slots> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Open a waiter for `request`, asked by `session_id`'s hook. Re-opening a request id replaces
    /// the earlier waiter.
    pub fn open(&self, session_id: &str, request: PermissionRequestView) {
        let mut slots = self.slots();
        let seq = slots.next_seq;
        slots.next_seq += 1;
        let request_id = request.request_id.clone();
        slots.waiters.remove(&request_id);
        slots.delivered.remove(&request_id);
        slots.open.insert(
            request_id,
            Opened {
                seq,
                open: OpenRequest {
                    session_id: session_id.to_string(),
                    request,
                },
            },
        );
    }

    /// The request a hook holds open under `request_id`, if any.
    #[must_use]
    pub fn open_request(&self, request_id: &str) -> Option<OpenRequest> {
        self.slots()
            .open
            .get(request_id)
            .map(|opened| opened.open.clone())
    }

    /// Wait until the waiter `request_id` names is answered, or `deadline` passes. A request no
    /// waiter is open for times out at once.
    pub async fn wait(&self, request_id: &str, deadline: Duration) -> WaitOutcome {
        let (seq, mut woken) = {
            let mut slots = self.slots();
            if let Some(delivered) = slots.delivered.remove(request_id) {
                return WaitOutcome::Answered(delivered);
            }
            let Some(seq) = slots.open.get(request_id).map(|opened| opened.seq) else {
                return WaitOutcome::TimedOut;
            };
            let (wake, woken) = oneshot::channel();
            slots.waiters.insert(request_id.to_string(), wake);
            (seq, woken)
        };
        if let Ok(Ok(delivered)) = tokio::time::timeout(deadline, &mut woken).await {
            return WaitOutcome::Answered(delivered);
        }
        // The deadline passed (or a re-open dropped the waiter). An answer may have landed between
        // the deadline and this lock: it was sent under the same lock, so it is in the channel.
        let mut slots = self.slots();
        if let Ok(delivered) = woken.try_recv() {
            return WaitOutcome::Answered(delivered);
        }
        // Close the request only if it is still the one this wait opened on: a re-open replaced it
        // (and dropped this wait's wake-up), and the new waiter is not this wait's to close.
        if slots.is_open_as(request_id, seq) {
            slots.waiters.remove(request_id);
            slots.open.remove(request_id);
        }
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
        let request_id = answer.request_id();
        let mut slots = self.slots();
        if let Some(first) = slots.answered.get(request_id) {
            return Err(format!(
                "permission request '{request_id}' was already answered by {first}"
            ));
        }
        if slots.open.remove(request_id).is_none() {
            return Err(format!(
                "no hook is waiting on permission request '{request_id}'"
            ));
        }
        let delivered = DeliveredAnswer {
            answer: answer.clone(),
            by: by.clone(),
            message,
        };
        slots.remember_answer(request_id, by);
        match slots.waiters.remove(request_id) {
            Some(wake) => {
                // A dropped receiver is a wait that ended; the answer is recorded all the same.
                let _ = wake.send(delivered);
            }
            None => {
                slots.delivered.insert(request_id.to_string(), delivered);
            }
        }
        Ok(())
    }
}

impl PermissionAnswering for PendingAnswers {
    fn answer_as(&self, answer: &PermissionAnswer, by: AnsweredBy) -> Result<(), String> {
        self.answer_with_message(answer, by, None)
    }

    fn answered_by(&self, request_id: &str) -> Option<AnsweredBy> {
        self.slots().answered.get(request_id).cloned()
    }

    /// Never: a claude-cli child's request is the orchestrator's to answer, and a person answers it
    /// in Claude's own terminal, not through this registry.
    fn deferred_to_user(&self, _request_id: &str) -> bool {
        false
    }

    /// The earliest-opened request a hook is still waiting on, across this daemon's sessions.
    fn pending_request(&self) -> Option<PermissionRequestView> {
        self.slots()
            .open
            .values()
            .min_by_key(|opened| opened.seq)
            .map(|opened| opened.open.request.clone())
    }
}
