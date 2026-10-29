//! An orchestrator answering its child's `AskUserQuestion` (`#home-inbox` 8/9).
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

use std::collections::HashSet;
use std::path::Path;

use serde_json::{json, Value};
use tddy_core::agent_activity::{append_agent_activity, AgentActivityRecord, STATUS_COMPLETED};
use tddy_core::pending_question::{clear_pending_question, read_pending_question, PendingQuestion};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_discovery::subagent::AnsweredBy;

use super::{require_orchestrator, AnswerContext, AnswerRefusal, AnsweringCaller};

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
    format!("question:{request_id}")
}

/// What the child's activity records as the output of that call: `{"answeredBy": <session id>,
/// "answers": [{"questionIndex", "selected", "other"?}]}` — `other` left out when absent, as `6/8`'s
/// `{"answeredBy", "optionId"}` is for a permission.
#[must_use]
pub fn answered_question_output(answered_by: &str, answers: &[QuestionAnswer]) -> Value {
    let answers: Vec<Value> = answers
        .iter()
        .map(|answer| {
            let mut entry = json!({
                "questionIndex": answer.question_index,
                "selected": answer.selected,
            });
            if let Some(other) = &answer.other {
                entry["other"] = json!(other);
            }
            entry
        })
        .collect();
    json!({ "answeredBy": answered_by, "answers": answers })
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
    let asked = question.questions.len();
    let mut answered = HashSet::new();
    for answer in answers {
        let index = answer.question_index as usize;
        let Some(item) = question.questions.get(index) else {
            return Err(format!(
                "question {index} does not exist: the call asks {asked}"
            ));
        };
        if !answered.insert(index) {
            return Err(format!("question {index} is answered twice"));
        }
        if let Some(label) = answer
            .selected
            .iter()
            .find(|label| !item.options.iter().any(|option| &option.label == *label))
        {
            return Err(format!("question {index} does not offer {label:?}"));
        }
        let free_text = answer.other.as_deref().is_some_and(|text| !text.is_empty());
        let picks = answer.selected.len() + usize::from(free_text);
        if !item.multi_select && picks > 1 {
            return Err(format!(
                "question {index} is a single choice: give one answer, not {picks}"
            ));
        }
        if picks == 0 {
            return Err(format!(
                "question {index} selects nothing: give a label or other"
            ));
        }
    }
    Ok(())
}

/// Deliver `caller`'s `answers` to the question `target`'s hook waits on under `request_id`. See the
/// module doc for the checks, in order.
///
/// # Errors
///
/// The [`AnswerRefusal`] of the first check that fails; the waiting hook keeps waiting.
pub fn answer_question(
    context: &AnswerContext<'_>,
    caller: &AnsweringCaller,
    target: &str,
    request_id: &str,
    answers: Vec<QuestionAnswer>,
) -> Result<AnsweredQuestion, AnswerRefusal> {
    let session_dir = unified_session_dir_path(context.sessions_base, target);

    // 1. The caller is the child's orchestrator, on the host the changeset records.
    require_orchestrator(context, caller, target, &session_dir)?;

    // 2. Nobody answered it first.
    if let Some(first) = context.pending.question_answered_by(request_id) {
        return Err(AnswerRefusal::AlreadyAnswered {
            request_id: request_id.to_string(),
            by: first.name(),
        });
    }

    // 3 and 4. The child waits on exactly this question, which carries a request id, and it is a
    // question.
    let question = read_pending_question(&session_dir)
        .map_err(|e| AnswerRefusal::Io(format!("cannot read the pending question: {e}")))?
        .ok_or_else(|| AnswerRefusal::UnknownRequest {
            request_id: request_id.to_string(),
        })?;
    if question.request_id.is_empty() {
        return Err(AnswerRefusal::NoRequestId {
            session_id: target.to_string(),
        });
    }
    if question.request_id != request_id {
        return Err(AnswerRefusal::UnknownRequest {
            request_id: request_id.to_string(),
        });
    }
    if question.kind == "permission" {
        return Err(AnswerRefusal::NotAQuestion {
            request_id: request_id.to_string(),
            kind: question.kind,
        });
    }

    // 5. A hook is waiting on it, and the answers fit the questions it asked.
    let open = context
        .pending
        .open_question_request(request_id)
        .filter(|open| open.session_id == target)
        .ok_or_else(|| AnswerRefusal::UnknownRequest {
            request_id: request_id.to_string(),
        })?;
    validate_question_answers(&open.question, &answers).map_err(|reason| {
        AnswerRefusal::InvalidAnswers {
            request_id: request_id.to_string(),
            reason,
        }
    })?;

    let by = AnsweredBy::Orchestrator(caller.session_id.clone());
    // The registry decides the race check 5 cannot: another answer, or the wait's deadline, landing
    // since.
    if context
        .pending
        .answer_question_with(request_id, answers.clone(), by.clone())
        .is_err()
    {
        return Err(match context.pending.question_answered_by(request_id) {
            Some(first) => AnswerRefusal::AlreadyAnswered {
                request_id: request_id.to_string(),
                by: first.name(),
            },
            None => AnswerRefusal::UnknownRequest {
                request_id: request_id.to_string(),
            },
        });
    }

    let at_unix_ms = tddy_daemon_kernel::now_unix_ms();
    record_answer(&session_dir, &question, &answers, &by, at_unix_ms);
    // Clear only the question that was answered: the hook has returned, so Claude may already have
    // recorded its next one, and that one is not this answer's to clear.
    let still_pending = matches!(
        read_pending_question(&session_dir),
        Ok(Some(current)) if current.request_id == request_id
    );
    if still_pending {
        if let Err(e) = clear_pending_question(&session_dir) {
            log::warn!(
                target: "tddy_session_activity::answers",
                "answered question {request_id}, but could not clear its pending question: {e}"
            );
        }
    }

    Ok(AnsweredQuestion {
        session_id: target.to_string(),
        request_id: request_id.to_string(),
        answered_by: caller.session_id.clone(),
        answers,
        at_unix_ms,
    })
}

/// Record in the child's activity who answered what. The answer is already delivered, so a failed
/// write is logged and not turned into a refusal.
fn record_answer(
    session_dir: &Path,
    question: &PendingQuestion,
    answers: &[QuestionAnswer],
    by: &AnsweredBy,
    at_unix_ms: u64,
) {
    let record = AgentActivityRecord {
        call_id: question_tool_call_id(&question.request_id),
        tool_name: question.tool_name.clone(),
        input: serde_json::from_str(&question.tool_input_json).unwrap_or(Value::Null),
        status: STATUS_COMPLETED.to_string(),
        result: answered_question_output(&by.name(), answers),
        error_message: String::new(),
        started_unix_ms: question.at_unix_ms,
        completed_unix_ms: at_unix_ms,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: 0,
        changed_paths: Vec::new(),
    };
    if let Err(e) = append_agent_activity(session_dir, &record) {
        log::warn!(
            target: "tddy_session_activity::answers",
            "could not record the answer to question {}: {e}",
            question.request_id
        );
    }
}
