//! A coding subagent's permission request, yielded to the caller, and the caller's answer.
//!
//! A coding agent asks before it edits or runs something its policy does not pre-approve. That
//! question is the invoking agent's to answer: the turn ends with
//! [`super::StopReason::PermissionRequested`] and the request, and `subagent_resume { permission }`
//! delivers the answer to the same, still-running agent turn.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § A coding subagent's permission request is the caller's to answer

/// The tool call the agent asks permission for.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionToolCall {
    pub title: String,
    /// ACP's tool kind (`edit`, `execute`, `read`, …); `other` when the agent named none.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<serde_json::Value>,
}

/// One answer the agent offers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionChoice {
    pub option_id: String,
    pub name: String,
    /// `allow_once`, `allow_always`, `reject_once` or `reject_always`.
    pub kind: String,
}

/// A permission request pending on a conversation, as the caller reads it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequestView {
    /// Names this request; a resume answering it must name it back.
    pub request_id: String,
    pub tool_call: PermissionToolCall,
    pub options: Vec<PermissionChoice>,
}

/// The caller's answer to a pending [`PermissionRequestView`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "answer"
)]
pub enum PermissionAnswer {
    /// One of the request's options, by id.
    Selected {
        request_id: String,
        option_id: String,
    },
    /// The request answered as cancelled.
    Cancelled { request_id: String },
}

impl PermissionAnswer {
    /// The request this answers.
    pub fn request_id(&self) -> &str {
        match self {
            Self::Selected { request_id, .. } | Self::Cancelled { request_id } => request_id,
        }
    }
}

/// The `tool_call_id` a transcript records a permission request under: the request's id behind the
/// `permission:` prefix the web reads it by.
pub fn permission_tool_call_id(request_id: &str) -> String {
    format!("permission:{request_id}")
}

/// The name the main agent answers under: the `answeredBy` a record names it by.
pub const MAIN_AGENT_NAME: &str = "main-agent";

/// The title a request's transcript entry carries, naming the tool the request asks to run.
pub fn permission_title(request: &PermissionRequestView) -> String {
    format!("Permission: {}", request.tool_call.title)
}

/// The refusal a main-agent answer receives while the turn holding its request is still running: the answer reached
/// the request, and the turn carries on, with its outcome going to the call that deferred the request.
pub fn answer_delivered_while_held(request_id: &str) -> String {
    format!(
        "permission request '{request_id}' was answered and the answer was delivered to the coding agent; \
         the turn holding it carries on, and its outcome goes to the call that deferred the request, \
         which collects it with subagent_await"
    )
}

/// The raw output of an answered request's record, naming who answered it: `{answeredBy, optionId}` for a chosen
/// option, `{answeredBy, cancelled: true}` for a cancellation. The one shape every answered record carries, whether
/// the answer was given or the request was cancelled.
pub fn answered_output(answered_by: &str, answer: &PermissionAnswer) -> serde_json::Value {
    match answer {
        PermissionAnswer::Selected { option_id, .. } => {
            serde_json::json!({ "answeredBy": answered_by, "optionId": option_id })
        }
        PermissionAnswer::Cancelled { .. } => {
            serde_json::json!({ "answeredBy": answered_by, "cancelled": true })
        }
    }
}

/// The input a request's transcript entry carries: the request as the caller reads it, its `title` at the top
/// level, and whether the main agent handed it to the user. A request the main agent answers itself is recorded
/// with `deferred_to_user` false.
pub fn request_input_json(
    request: &PermissionRequestView,
    deferred_to_user: bool,
) -> serde_json::Value {
    serde_json::json!({
        "requestId": request.request_id,
        "title": request.tool_call.title,
        "toolCall": request.tool_call,
        "options": request.options,
        "deferredToUser": deferred_to_user,
    })
}

/// Who answered a parked permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnsweredBy {
    /// The main agent, through `subagent_resume`.
    MainAgent,
    /// A user, by login, through `AnswerAgentPermission`.
    User(String),
}

impl AnsweredBy {
    /// The name a record gives as `answeredBy`: `main-agent`, or the user's login.
    pub fn name(&self) -> String {
        match self {
            Self::MainAgent => MAIN_AGENT_NAME.to_string(),
            Self::User(login) => login.clone(),
        }
    }
}

impl std::fmt::Display for AnsweredBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MainAgent => write!(f, "the main agent"),
            Self::User(login) => write!(f, "{login}"),
        }
    }
}

/// Answering a parked permission request from either side, once: the first answer wins, and a later
/// one is refused naming who answered first.
pub trait PermissionAnswering: Send + Sync {
    /// Deliver `answer` to the request it names, recording `by` as who answered it. Refused, and not
    /// delivered, when that request was already answered.
    fn answer_as(&self, answer: &PermissionAnswer, by: AnsweredBy) -> Result<(), String>;

    /// Who answered `request_id`, once it has been answered.
    fn answered_by(&self, request_id: &str) -> Option<AnsweredBy>;

    /// Whether `request_id` was handed to the user, so that a user may answer it.
    fn deferred_to_user(&self, request_id: &str) -> bool;

    /// The request a turn is waiting on, if any: the one a cancel of the conversation leaves unanswered, so its
    /// record can name who cancelled it.
    fn pending_request(&self) -> Option<PermissionRequestView>;
}
