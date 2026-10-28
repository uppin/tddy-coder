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

use std::collections::HashMap;
use std::time::Duration;

use tddy_core::pending_question::PendingQuestion;
use tddy_discovery::subagent::AnsweredBy;

use super::PendingAnswers;
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

/// What the question waiters hold, behind [`PendingAnswers`]' one lock for them.
// TODO(question-answer): read by the implementation; the stubs never do.
#[allow(dead_code)]
#[derive(Default)]
pub(super) struct QuestionSlots {
    /// Questions a hook is waiting on, by `request_id`.
    open: HashMap<String, OpenQuestion>,
    /// Questions already answered, by `request_id`, with who answered first.
    answered: HashMap<String, AnsweredBy>,
}

impl PendingAnswers {
    /// Open a waiter for `question`, asked by `session_id`'s hook, under `question.request_id`.
    /// Re-opening a request id replaces the earlier waiter.
    pub fn open_question(&self, session_id: &str, question: PendingQuestion) {
        // TODO(question-answer): implement — insert into `questions.open`.
        let _ = (&self.questions, session_id, question);
    }

    /// The question a hook holds open under `request_id`, if any.
    #[must_use]
    pub fn open_question_request(&self, request_id: &str) -> Option<OpenQuestion> {
        // TODO(question-answer): implement — look `request_id` up in `questions.open`.
        let _ = (&self.questions, request_id);
        None
    }

    /// Wait until the question waiter `request_id` names is answered, or `deadline` passes. A
    /// request no waiter is open for times out at once.
    pub async fn wait_for_question(
        &self,
        request_id: &str,
        deadline: Duration,
    ) -> QuestionWaitOutcome {
        // TODO(question-answer): implement — a oneshot per waiter, raced against `deadline`; on
        // timeout the waiter is removed.
        let _ = (&self.questions, request_id, deadline);
        QuestionWaitOutcome::TimedOut
    }

    /// Deliver `answers` to the waiter `request_id` names, recording `by` as who answered. The first
    /// answer wins: a request already answered is refused as `question request '<id>' was already
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
        // TODO(question-answer): implement — refuse when `questions.answered` names the request,
        // then take the waiter out of `questions.open`, record `by` and wake it.
        let _ = (&self.questions, answers, by);
        Err(format!(
            "question request '{request_id}': answering is not implemented"
        ))
    }

    /// Who answered the question `request_id`, once it has been answered.
    #[must_use]
    pub fn question_answered_by(&self, request_id: &str) -> Option<AnsweredBy> {
        // TODO(question-answer): implement — look `request_id` up in `questions.answered`.
        let _ = (&self.questions, request_id);
        None
    }
}
