//! An orchestrator answering its child's `AskUserQuestion` (`#home-inbox` 8/8).
//!
//! [`answer_question`] is what `AnswerSessionQuestion` runs on the child's host. It refuses unless
//! every one of these holds, in this order — the first three are [`super::answer_permission`]'s:
//!
//! 1. the child's changeset `orchestrator_session_id` names the caller;
//! 2. the request was not answered already (a second answer is refused naming the first answerer);
//! 3. the child's pending question carries a `request_id` — a `tool` session's clarification does
//!    not ([`AnswerRefusal::NoRequestId`]) — and it is open under the one asked for;
//! 4. its kind is a question, not `permission` ([`AnswerRefusal::NotAQuestion`], naming
//!    `AnswerSessionPermission`);
//! 5. [`validate_question_answers`] accepts the answers against the recorded questions, and a hook
//!    is waiting on them.
//!
//! It then wakes the hook with the answers, records who answered, when and what in the child's
//! activity log, and clears the pending question. Every refusal leaves the waiting hook waiting.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use tddy_core::pending_question::PendingQuestion;

use super::{AnswerContext, AnswerRefusal, AnsweringCaller};

/// One answer to one question of an `AskUserQuestion` call: `question_index` (from 0),
/// `selected` labels, and `other` free text. The wire type, so the hook, the registry and the tool
/// speak one shape.
pub use tddy_service::proto::activity::QuestionAnswer;

/// The tool a permission (not a question) is answered with, named by [`AnswerRefusal::NotAQuestion`].
pub const ANSWER_SESSION_PERMISSION: &str = "AnswerSessionPermission";

/// The `tool_call_id` the child's activity records an answer to `request_id` under:
/// `question:<request_id>`, beside `6/8`'s `permission:<request_id>`.
#[must_use]
pub fn question_tool_call_id(request_id: &str) -> String {
    // TODO(question-answer): implement
    let _ = request_id;
    String::new()
}

/// What the child's activity records as the output of that call: `{"answeredBy": <session id>,
/// "answers": [{"questionIndex", "selected", "other"?}]}` — `other` left out when absent, as `6/8`'s
/// `{"answeredBy", "optionId"}` is for a permission.
#[must_use]
pub fn answered_question_output(
    answered_by: &str,
    answers: &[QuestionAnswer],
) -> serde_json::Value {
    // TODO(question-answer): implement
    let _ = (answered_by, answers);
    serde_json::Value::Null
}

/// An answer that was delivered.
#[derive(Debug, Clone, PartialEq)]
pub struct AnsweredQuestion {
    pub session_id: String,
    pub request_id: String,
    /// The orchestrator's session id.
    pub answered_by: String,
    pub answers: Vec<QuestionAnswer>,
    pub at_unix_ms: u64,
}

/// Whether `answers` answer `question`'s recorded `questions`; otherwise why not, in words an
/// orchestrator can act on.
///
/// Refused, each naming what was wrong:
/// - an index the call has no question at (the reason names the index and how many questions
///   there are);
/// - a question answered twice;
/// - a label the question does not offer, exactly as spelled (an `other` text never stands in for
///   it: free text goes in `other`);
/// - more than one answer to a single-choice question — a selection and `other` count one each
///   (the reason says "one");
/// - an answer that selects nothing and gives no `other`.
///
/// `other` is always accepted as free text, as Claude's "Other" is. A question the answers leave
/// out is not refused: Claude shows the rest.
pub fn validate_question_answers(
    question: &PendingQuestion,
    answers: &[QuestionAnswer],
) -> Result<(), String> {
    // TODO(question-answer): implement — the refusals above, against `question.questions`.
    let _ = (question, answers);
    Err("validate_question_answers: not implemented".to_string())
}

/// Deliver `caller`'s `answers` to the question `target`'s hook waits on under `request_id`. See the
/// module doc for the checks, in order.
pub fn answer_question(
    context: &AnswerContext<'_>,
    caller: &AnsweringCaller,
    target: &str,
    request_id: &str,
    answers: Vec<QuestionAnswer>,
) -> Result<AnsweredQuestion, AnswerRefusal> {
    // TODO(question-answer): implement — the five checks, then `PendingAnswers::answer_question_with`,
    // the activity record of who answered what, and `clear_pending_question`.
    let _ = (context, caller, answers);
    Err(AnswerRefusal::UnknownRequest {
        request_id: format!(
            "{request_id} (on session {target}; answering a question is not implemented)"
        ),
    })
}
