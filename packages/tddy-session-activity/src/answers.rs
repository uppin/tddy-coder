//! An orchestrator answering its child's permission prompt (`#home-inbox` 6/8).
//!
//! [`answer_permission`] is what `AnswerSessionPermission` runs on the child's host. It refuses
//! unless every one of these holds, in this order:
//!
//! 1. the child's changeset `orchestrator_session_id` names the caller — a project-home session or a
//!    spawn-capable one, on the host the changeset records (`#start-parity` 2/9, 9/9);
//! 2. the request was not answered already (a second answer is refused naming the first answerer);
//! 3. the child's pending question (`#home-inbox` 3/8's record) is open under that `request_id`;
//! 4. its kind is `permission` — a question is refused naming `AnswerSessionQuestion` (`8/9`);
//! 5. the chosen option is one the request offers, and a hook is waiting on it.
//!
//! It then wakes the hook, records who answered, when and what in the child's activity log, and
//! clears the pending question. Once the hook is woken the answer is delivered: a failure to record
//! it or to clear the question is logged, never returned as a refusal — every refusal leaves the
//! hook waiting, and a refused answer the hook already acted on would mislead the orchestrator. The
//! child's next status report clears a question left behind (`#home-inbox` 3/8's clear rule).
//!
//! The request is offered in `#coagents`' shape, a [`PermissionRequestView`], with the options
//! [`ALLOW_ONCE`] and [`REJECT_ONCE`], plus [`ALLOW_ALWAYS`] when Claude sent permission
//! suggestions.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::path::Path;

use tddy_core::agent_activity::{append_agent_activity, AgentActivityRecord, STATUS_COMPLETED};
use tddy_core::pending_question::{clear_pending_question, read_pending_question, PendingQuestion};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{read_changeset, WorkflowError};
use tddy_discovery::subagent::{
    answered_output, permission_tool_call_id, request_input_json, AnsweredBy, PermissionAnswer,
    PermissionAnswering, PermissionChoice, PermissionRequestView, PermissionToolCall,
};

use crate::pending_answers::PendingAnswers;

mod question;

// `#home-inbox` 8/9: answering a question (`AskUserQuestion`) is `question`'s, beside the permission
// answer above; one path, `answers::answer_question`.
pub use question::{
    answer_question, answered_question_output, question_tool_call_id, validate_question_answers,
    AnsweredQuestion, QuestionAnswer, ANSWER_SESSION_PERMISSION,
};

/// Allow this one call: Claude's `allow`.
pub const ALLOW_ONCE: &str = "allow_once";
/// Allow, and apply Claude's permission suggestions so it does not ask again: Claude's `allow`
/// with `updatedPermissions`. Offered only when Claude sent suggestions.
pub const ALLOW_ALWAYS: &str = "allow_always";
/// Refuse this one call: Claude's `deny`, with the answer's message.
pub const REJECT_ONCE: &str = "reject_once";

/// The tool a question (not a permission) is answered with, named by the refusal.
pub const ANSWER_SESSION_QUESTION: &str = "AnswerSessionQuestion";

/// The session answering, as its own host confirms it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsweringCaller {
    pub session_id: String,
    /// The daemon that runs the caller; compared with the child's
    /// `orchestrator_daemon_instance_id` (absent there = the child's own host).
    pub daemon_instance_id: String,
}

/// Where the child lives and the registry its hook waits in.
pub struct AnswerContext<'a> {
    /// The child's OS user's sessions base on this host.
    pub sessions_base: &'a Path,
    /// This host's instance id, the one an absent `orchestrator_daemon_instance_id` means.
    pub this_daemon_instance_id: &'a str,
    pub pending: &'a PendingAnswers,
}

/// An answer that was delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsweredPermission {
    pub session_id: String,
    pub request_id: String,
    /// The orchestrator's session id.
    pub answered_by: String,
    /// The chosen option; empty when cancelled.
    pub option_id: String,
    pub cancelled: bool,
    pub at_unix_ms: u64,
}

/// Why an answer was refused. Every refusal leaves the waiting hook waiting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerRefusal {
    /// The caller is not the child's orchestrator: permission denied.
    NotTheOrchestrator {
        caller: String,
        target: String,
        /// Who is, when the child has one.
        orchestrator: Option<String>,
    },
    /// The request was answered already, by `by`.
    AlreadyAnswered { request_id: String, by: String },
    /// The child waits on no question under that id, or no hook is waiting on it any more (its
    /// wait timed out, and the person answers in Claude's terminal).
    UnknownRequest { request_id: String },
    /// The pending question is a question, not a permission.
    NotAPermission { request_id: String, kind: String },
    /// The option is not one the request offers.
    UnknownOption {
        request_id: String,
        option_id: String,
    },
    /// The pending question is a permission, not a question (`#home-inbox` 8/9).
    NotAQuestion { request_id: String, kind: String },
    /// The answers do not fit the question: an index it does not have, a label it does not offer, or
    /// several selections on a single-choice question (`#home-inbox` 8/9). `reason` says which.
    InvalidAnswers { request_id: String, reason: String },
    /// The child's pending question carries no `request_id` — a `tool` session's clarification,
    /// which is answered through its presenter, not here (`#home-inbox` 8/9).
    NoRequestId { session_id: String },
    /// The child's records could not be read, so nothing was delivered.
    Io(String),
}

impl std::fmt::Display for AnswerRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotTheOrchestrator {
                caller,
                target,
                orchestrator,
            } => write!(
                f,
                "permission denied: session {caller} is not the orchestrator of session {target} \
                 (its orchestrator is {})",
                orchestrator.as_deref().unwrap_or("none")
            ),
            Self::AlreadyAnswered { request_id, by } => {
                write!(
                    f,
                    "permission request '{request_id}' was already answered by {by}"
                )
            }
            Self::UnknownRequest { request_id } => write!(
                f,
                "no permission request '{request_id}' is pending on that session"
            ),
            Self::NotAPermission { request_id, kind } => write!(
                f,
                "pending question '{request_id}' is a {kind} question, not a permission: \
                 answer it with {ANSWER_SESSION_QUESTION}"
            ),
            Self::UnknownOption {
                request_id,
                option_id,
            } => write!(
                f,
                "permission request '{request_id}' offers no option '{option_id}'"
            ),
            Self::NotAQuestion { request_id, kind } => write!(
                f,
                "pending question '{request_id}' is a {kind} request, not a question: \
                 answer it with {ANSWER_SESSION_PERMISSION}"
            ),
            Self::InvalidAnswers { request_id, reason } => {
                write!(f, "answers to question '{request_id}' refused: {reason}")
            }
            Self::NoRequestId { session_id } => write!(
                f,
                "session {session_id}'s pending question carries no request_id: it is answered \
                 through the session's own presenter, not with {ANSWER_SESSION_QUESTION}"
            ),
            Self::Io(reason) => write!(f, "{reason}"),
        }
    }
}

/// `question` as an orchestrator reads it.
///
/// - `tool_call.title` is the tool's name; `kind` is ACP's name for it — `execute` for `Bash`, `edit`
///   for `Edit`, `MultiEdit`, `Write` and `NotebookEdit`, `read` for `Read`, `Glob` and `Grep`,
///   `other` for anything else; `raw_input` is the tool input parsed, or its text when it is not JSON.
/// - `options`, in order: [`ALLOW_ONCE`] ("Allow once"), [`ALLOW_ALWAYS`] ("Allow always", only when
///   `has_permission_suggestions`), [`REJECT_ONCE`] ("Reject"); each option's `kind` is its id.
#[must_use]
pub fn permission_request_view(
    question: &PendingQuestion,
    has_permission_suggestions: bool,
) -> PermissionRequestView {
    let raw_input = match question.tool_input_json.as_str() {
        "" => None,
        text => Some(
            serde_json::from_str(text)
                .unwrap_or_else(|_| serde_json::Value::String(text.to_string())),
        ),
    };
    let mut options = vec![choice(ALLOW_ONCE, "Allow once")];
    if has_permission_suggestions {
        options.push(choice(ALLOW_ALWAYS, "Allow always"));
    }
    options.push(choice(REJECT_ONCE, "Reject"));
    PermissionRequestView {
        request_id: question.request_id.clone(),
        tool_call: PermissionToolCall {
            title: question.tool_name.clone(),
            kind: acp_tool_kind(&question.tool_name).to_string(),
            raw_input,
        },
        options,
    }
}

/// An offered option whose `kind` is its id.
fn choice(option_id: &str, name: &str) -> PermissionChoice {
    PermissionChoice {
        option_id: option_id.to_string(),
        name: name.to_string(),
        kind: option_id.to_string(),
    }
}

/// ACP's name for a Claude tool.
fn acp_tool_kind(tool_name: &str) -> &'static str {
    match tool_name {
        "Bash" => "execute",
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => "edit",
        "Read" | "Glob" | "Grep" => "read",
        _ => "other",
    }
}

/// Deliver `caller`'s `answer` to the permission prompt `target`'s hook waits on, with `message`
/// for a `reject_*` option. See the module doc for the checks, in order.
///
/// # Errors
///
/// The [`AnswerRefusal`] of the first check that fails; the waiting hook keeps waiting.
pub fn answer_permission(
    context: &AnswerContext<'_>,
    caller: &AnsweringCaller,
    target: &str,
    answer: PermissionAnswer,
    message: Option<String>,
) -> Result<AnsweredPermission, AnswerRefusal> {
    let request_id = answer.request_id().to_string();
    let session_dir = unified_session_dir_path(context.sessions_base, target);

    // 1. The caller is the child's orchestrator, on the host the changeset records.
    require_orchestrator(context, caller, target, &session_dir)?;

    // 2. Nobody answered it first.
    if let Some(first) = context.pending.answered_by(&request_id) {
        return Err(AnswerRefusal::AlreadyAnswered {
            request_id,
            by: first.name(),
        });
    }

    // 3 and 4. The child waits on exactly this question, and it is a permission.
    let question = read_pending_question(&session_dir)
        .map_err(|e| AnswerRefusal::Io(format!("cannot read the pending question: {e}")))?
        .filter(|question| question.request_id == request_id)
        .ok_or_else(|| AnswerRefusal::UnknownRequest {
            request_id: request_id.clone(),
        })?;
    if question.kind != "permission" {
        return Err(AnswerRefusal::NotAPermission {
            request_id,
            kind: question.kind,
        });
    }

    // 5. A hook is waiting, and the option is one it was offered.
    let open = context
        .pending
        .open_request(&request_id)
        .filter(|open| open.session_id == target)
        .ok_or_else(|| AnswerRefusal::UnknownRequest {
            request_id: request_id.clone(),
        })?;
    let option_id = offered_option(&open.request, &answer)?;

    let by = AnsweredBy::Orchestrator(caller.session_id.clone());
    // The registry decides the race checks 2 and 5 cannot: another answer, or the wait's deadline,
    // landing since.
    if context
        .pending
        .answer_with_message(&answer, by.clone(), message)
        .is_err()
    {
        return Err(match context.pending.answered_by(&request_id) {
            Some(first) => AnswerRefusal::AlreadyAnswered {
                request_id,
                by: first.name(),
            },
            None => AnswerRefusal::UnknownRequest { request_id },
        });
    }

    let at_unix_ms = tddy_daemon_kernel::now_unix_ms();
    record_answer(
        &session_dir,
        &question,
        &open.request,
        &answer,
        &by,
        at_unix_ms,
    );
    clear_answered_pending_question(&session_dir, &request_id);

    Ok(AnsweredPermission {
        session_id: target.to_string(),
        request_id,
        answered_by: caller.session_id.clone(),
        option_id,
        cancelled: matches!(answer, PermissionAnswer::Cancelled { .. }),
        at_unix_ms,
    })
}

/// Clear the pending question in `session_dir` once `request_id` has been answered — only when it is
/// still the question recorded there. The hook has already returned, so Claude may have recorded its
/// next question, and that one is not this answer's to clear (`#home-inbox` 11/11; `8/9` fixed the
/// same race in [`answer_question`]).
///
/// A failure to read or clear is logged, never returned: the answer is already delivered.
pub fn clear_answered_pending_question(session_dir: &Path, request_id: &str) {
    let still_pending = matches!(
        read_pending_question(session_dir),
        Ok(Some(current)) if current.request_id == request_id
    );
    if !still_pending {
        return;
    }
    if let Err(e) = clear_pending_question(session_dir) {
        log::warn!(
            target: "tddy_session_activity::answers",
            "answered request {request_id}, but could not clear its pending question: {e}"
        );
    }
}

/// Refuse unless `caller` is `target`'s orchestrator: the session its changeset's
/// `orchestrator_session_id` names, on the host its `orchestrator_daemon_instance_id` names (this host
/// when absent).
pub(crate) fn require_orchestrator(
    context: &AnswerContext<'_>,
    caller: &AnsweringCaller,
    target: &str,
    session_dir: &Path,
) -> Result<(), AnswerRefusal> {
    let changeset = match read_changeset(session_dir) {
        Ok(changeset) => Some(changeset),
        Err(WorkflowError::ChangesetMissing(_)) => None,
        Err(e) => return Err(AnswerRefusal::Io(format!("cannot read the changeset: {e}"))),
    };
    let orchestrator = changeset
        .as_ref()
        .and_then(|changeset| changeset.orchestrator_session_id.clone());
    let orchestrator_host = changeset
        .as_ref()
        .and_then(|changeset| changeset.orchestrator_daemon_instance_id.as_deref())
        .filter(|host| !host.is_empty())
        .unwrap_or(context.this_daemon_instance_id);
    let is_orchestrator = orchestrator.as_deref() == Some(caller.session_id.as_str())
        && orchestrator_host == caller.daemon_instance_id;
    if is_orchestrator {
        Ok(())
    } else {
        Err(AnswerRefusal::NotTheOrchestrator {
            caller: caller.session_id.clone(),
            target: target.to_string(),
            orchestrator,
        })
    }
}

/// The option `answer` chose, when `request` offers it; empty for a cancellation.
fn offered_option(
    request: &PermissionRequestView,
    answer: &PermissionAnswer,
) -> Result<String, AnswerRefusal> {
    match answer {
        PermissionAnswer::Selected {
            request_id,
            option_id,
        } => {
            if request
                .options
                .iter()
                .any(|offered| &offered.option_id == option_id)
            {
                Ok(option_id.clone())
            } else {
                Err(AnswerRefusal::UnknownOption {
                    request_id: request_id.clone(),
                    option_id: option_id.clone(),
                })
            }
        }
        PermissionAnswer::Cancelled { .. } => Ok(String::new()),
    }
}

/// Record in the child's activity who answered what. The answer is already delivered, so a failed
/// write is logged and not turned into a refusal.
fn record_answer(
    session_dir: &Path,
    question: &PendingQuestion,
    request: &PermissionRequestView,
    answer: &PermissionAnswer,
    by: &AnsweredBy,
    at_unix_ms: u64,
) {
    let record = AgentActivityRecord {
        call_id: permission_tool_call_id(&question.request_id),
        tool_name: question.tool_name.clone(),
        input: request_input_json(request, false),
        status: STATUS_COMPLETED.to_string(),
        result: answered_output(&by.name(), answer),
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
            "could not record the answer to permission request {}: {e}",
            question.request_id
        );
    }
}
