//! An orchestrator answering its child's permission prompt (`#home-inbox` 6/8).
//!
//! [`answer_permission`] is what `AnswerSessionPermission` runs on the child's host. It refuses
//! unless every one of these holds, in this order:
//!
//! 1. the child's changeset `orchestrator_session_id` names the caller — a project-home session or a
//!    spawn-capable one, on the host the changeset records (`#start-parity` 2/9, 9/9);
//! 2. the request was not answered already (a second answer is refused naming the first answerer);
//! 3. the child's pending question (`#home-inbox` 3/8's record) is open under that `request_id`;
//! 4. its kind is `permission` — a question is refused naming `AnswerSessionQuestion` (`8/8`);
//! 5. the chosen option is one the request offers, and a hook is waiting on it.
//!
//! It then wakes the hook, records who answered, when and what in the child's activity log, and
//! clears the pending question.
//!
//! The request is offered in `#coagents`' shape, a [`PermissionRequestView`], with the options
//! [`ALLOW_ONCE`] and [`REJECT_ONCE`], plus [`ALLOW_ALWAYS`] when Claude sent permission
//! suggestions.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::path::Path;

use tddy_core::pending_question::PendingQuestion;
use tddy_discovery::subagent::{PermissionAnswer, PermissionRequestView};

use crate::pending_answers::PendingAnswers;

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
    /// The child's records could not be read or written.
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
    // TODO(permission-answer): implement — the tool call from `tool_name` / `tool_input_json`, and
    // the offered options.
    let _ = has_permission_suggestions;
    PermissionRequestView {
        request_id: question.request_id.clone(),
        tool_call: tddy_discovery::subagent::PermissionToolCall {
            title: String::new(),
            kind: String::new(),
            raw_input: None,
        },
        options: Vec::new(),
    }
}

/// Deliver `caller`'s `answer` to the permission prompt `target`'s hook waits on, with `message`
/// for a `reject_*` option. See the module doc for the checks, in order.
pub fn answer_permission(
    context: &AnswerContext<'_>,
    caller: &AnsweringCaller,
    target: &str,
    answer: PermissionAnswer,
    message: Option<String>,
) -> Result<AnsweredPermission, AnswerRefusal> {
    // TODO(permission-answer): implement — the five checks, then `answer_with_message`, the
    // activity record (`permission_tool_call_id`, `answered_output`) and `clear_pending_question`.
    let _ = (context, caller, message);
    Err(AnswerRefusal::UnknownRequest {
        request_id: format!(
            "{} (on session {target}; answering is not implemented)",
            answer.request_id()
        ),
    })
}
