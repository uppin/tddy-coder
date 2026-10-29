//! The `AskUserQuestion` calls claude-cli hooks are holding open on this daemon, waiting for an
//! orchestrator's answers (`#home-inbox` 8/8).
//!
//! Beside the permission waiters of [`PendingAnswers`] (`6/8`), not in them: a permission is answered
//! with one option id, a question with an answer per question, and `#coagents`' `PermissionAnswer`
//! has no room for that. Same lifetime and rules — in memory, one waiter per `request_id`, a daemon
//! restart drops every waiter, the first answer wins and a second is refused naming the first
//! answerer.
//!
//! `AwaitQuestionAnswer` [`PendingAnswers::open_question`]s a waiter and
//! [`PendingAnswers::wait_for_question`]s on it; `AnswerSessionQuestion` delivers into it with
//! [`PendingAnswers::answer_question_with`].

use std::collections::{HashMap, VecDeque};
use std::sync::{MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::oneshot;

use tddy_core::pending_question::PendingQuestion;
use tddy_discovery::subagent::AnsweredBy;

use super::{PendingAnswers, ANSWERED_REQUESTS_KEPT};
use crate::answers::QuestionAnswer;

/// Answers as they reached a waiting hook, and who gave them.
#[derive(Debug, Clone, PartialEq)]
pub struct DeliveredQuestionAnswers {
    pub answers: Vec<QuestionAnswer>,
    pub by: AnsweredBy,
}

/// How a [`PendingAnswers::wait_for_question`] ended.
#[derive(Debug, Clone, PartialEq)]
pub enum QuestionWaitOutcome {
    Answered(DeliveredQuestionAnswers),
    /// Nobody answered before the deadline. The waiter is gone: a later answer is refused.
    TimedOut,
}

/// One open question waiter: the session whose hook holds it and the question as recorded.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenQuestion {
    pub session_id: String,
    pub question: PendingQuestion,
}

/// An open question and the order it was opened in.
struct OpenedQuestion {
    seq: u64,
    open: OpenQuestion,
}

/// What the question waiters hold, behind [`PendingAnswers`]' one lock for them.
#[derive(Default)]
pub(super) struct QuestionSlots {
    /// Questions a hook is waiting on, by `request_id`.
    open: HashMap<String, OpenedQuestion>,
    /// The wake-up of each question a hook is currently [`PendingAnswers::wait_for_question`]ing on.
    waiters: HashMap<String, oneshot::Sender<DeliveredQuestionAnswers>>,
    /// Answers delivered before their hook began waiting; the wait takes its own.
    delivered: HashMap<String, DeliveredQuestionAnswers>,
    /// Questions already answered, by `request_id`, with who answered first, for the last
    /// [`ANSWERED_REQUESTS_KEPT`] answers (`answered_order`).
    answered: HashMap<String, AnsweredBy>,
    /// The keys of `answered`, oldest first.
    answered_order: VecDeque<String>,
    /// The next [`OpenedQuestion::seq`].
    next_seq: u64,
}

impl QuestionSlots {
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

    /// Whether the open question under `request_id` is still the one opened as `seq`.
    fn is_open_as(&self, request_id: &str, seq: u64) -> bool {
        self.open
            .get(request_id)
            .is_some_and(|opened| opened.seq == seq)
    }
}

impl PendingAnswers {
    /// The question slots. A poisoned lock still holds consistent maps (every critical section is a
    /// few map operations), so it is recovered rather than turned into a panic in a hook's request.
    fn question_slots(&self) -> MutexGuard<'_, QuestionSlots> {
        self.questions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Open a waiter for `question`, asked by `session_id`'s hook, under `question.request_id`.
    /// Re-opening a request id replaces the earlier waiter.
    pub fn open_question(&self, session_id: &str, question: PendingQuestion) {
        let mut slots = self.question_slots();
        let seq = slots.next_seq;
        slots.next_seq += 1;
        let request_id = question.request_id.clone();
        slots.waiters.remove(&request_id);
        slots.delivered.remove(&request_id);
        slots.open.insert(
            request_id,
            OpenedQuestion {
                seq,
                open: OpenQuestion {
                    session_id: session_id.to_string(),
                    question,
                },
            },
        );
    }

    /// The question a hook holds open under `request_id`, if any.
    #[must_use]
    pub fn open_question_request(&self, request_id: &str) -> Option<OpenQuestion> {
        self.question_slots()
            .open
            .get(request_id)
            .map(|opened| opened.open.clone())
    }

    /// Wait until the question waiter `request_id` names is answered, or `deadline` passes. A
    /// request no waiter is open for times out at once.
    pub async fn wait_for_question(
        &self,
        request_id: &str,
        deadline: Duration,
    ) -> QuestionWaitOutcome {
        let (seq, mut woken) = {
            let mut slots = self.question_slots();
            if let Some(delivered) = slots.delivered.remove(request_id) {
                return QuestionWaitOutcome::Answered(delivered);
            }
            let Some(seq) = slots.open.get(request_id).map(|opened| opened.seq) else {
                return QuestionWaitOutcome::TimedOut;
            };
            let (wake, woken) = oneshot::channel();
            slots.waiters.insert(request_id.to_string(), wake);
            (seq, woken)
        };
        if let Ok(Ok(delivered)) = tokio::time::timeout(deadline, &mut woken).await {
            return QuestionWaitOutcome::Answered(delivered);
        }
        // The deadline passed (or a re-open dropped the waiter). An answer may have landed between
        // the deadline and this lock: it was sent under the same lock, so it is in the channel.
        let mut slots = self.question_slots();
        if let Ok(delivered) = woken.try_recv() {
            return QuestionWaitOutcome::Answered(delivered);
        }
        // Close the question only if it is still the one this wait opened on: a re-open replaced it
        // (and dropped this wait's wake-up), and the new waiter is not this wait's to close.
        if slots.is_open_as(request_id, seq) {
            slots.waiters.remove(request_id);
            slots.open.remove(request_id);
        }
        QuestionWaitOutcome::TimedOut
    }

    /// Deliver `answers` to the waiter `request_id` names, recording `by` as who answered. The first
    /// answer wins: a question already answered is refused as `question request '<id>' was already
    /// answered by <by>`, and one no hook is waiting on as `no hook is waiting on question request
    /// '<id>'`. Answers delivered before the hook calls [`Self::wait_for_question`] are that wait's
    /// outcome. The answers are not checked here: [`crate::answers::validate_question_answers`]
    /// already did.
    pub fn answer_question_with(
        &self,
        request_id: &str,
        answers: Vec<QuestionAnswer>,
        by: AnsweredBy,
    ) -> Result<(), String> {
        let mut slots = self.question_slots();
        if let Some(first) = slots.answered.get(request_id) {
            return Err(format!(
                "question request '{request_id}' was already answered by {first}"
            ));
        }
        if slots.open.remove(request_id).is_none() {
            return Err(format!(
                "no hook is waiting on question request '{request_id}'"
            ));
        }
        let delivered = DeliveredQuestionAnswers {
            answers,
            by: by.clone(),
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

    /// Who answered the question `request_id`, once it has been answered.
    #[must_use]
    pub fn question_answered_by(&self, request_id: &str) -> Option<AnsweredBy> {
        self.question_slots().answered.get(request_id).cloned()
    }
}
