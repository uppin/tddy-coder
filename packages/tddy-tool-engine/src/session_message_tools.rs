//! `PostSessionMessage`, `ReadSessionMessages`: sessions send each other messages, which the
//! recipient reads when it asks (`#activity-wake` 5/5).
//!
//! A message is never typed into a terminal: it is recorded as an activity record of the project
//! log, and delivered when the recipient looks — `ReadSessionMessages`, or its inbox watch
//! (`CreateSessionWatch` with the kind `message` alone), which `tddy-tools monitor --kinds message`
//! streams as `message` lines. `SendSessionMessage` is still the way to make a session act now.
//!
//! Offered to **every** session role. Who may address whom is the daemon's rule, read from the
//! sessions' records: on one host, within one project and one OS user, never oneself —
//!
//! - an orchestrator → any session it can see;
//! - a session → its orchestrator, and the project's main session;
//! - a reply → the sender of the message it replies to.
//!
//! The inbox itself is `tddy_session_activity::session_messages`'. That crate sits above this one,
//! so the port speaks in JSON values and plain arguments.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_worktree_service::session_target::HomeCaller;

use crate::catalog::ToolDef;
use crate::project_home_tools::tool_def;
use crate::ToolOutcome;

pub const POST_SESSION_MESSAGE: &str = "PostSessionMessage";
pub const READ_SESSION_MESSAGES: &str = "ReadSessionMessages";

/// The refusal of a port on a host that keeps no inboxes.
pub const NO_SESSION_MESSAGES: &str = "this host keeps no session messages";

/// Whether `name` is one of the two message tools.
pub fn is_session_message_tool(name: &str) -> bool {
    matches!(name, POST_SESSION_MESSAGE | READ_SESSION_MESSAGES)
}

/// What `PostSessionMessage` asks for, once its arguments are read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostMessageRequest {
    pub to_session_id: String,
    pub text: String,
    /// The message this one replies to: one the caller received.
    pub reply_to: Option<String>,
    /// The host the recipient runs on, as `ListProjectSessions` names it; `None` to have it found —
    /// on this host, as the caller's orchestrator's host, as the host of the message replied to, or
    /// by asking the connected hosts (n5).
    pub daemon_instance_id: Option<String>,
}

/// What `ReadSessionMessages` asks for, once its arguments are read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadMessagesRequest {
    /// Read after this message, read or not; `None` for the unread ones.
    pub after_message_id: Option<String>,
    pub limit: Option<usize>,
}

/// Who a caller may address, as its refusals name them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Addressable {
    /// The caller is an orchestrator: it may address any session of its project it can see.
    pub any_visible_session: bool,
    /// The orchestrator that started the caller.
    pub orchestrator_session_id: Option<String>,
    /// The project's main session, when it is not the caller.
    pub main_session_id: Option<String>,
}

impl std::fmt::Display for Addressable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.any_visible_session {
            return write!(
                f,
                "you may address any other session of this project on this host (ListProjectSessions)"
            );
        }
        let mut whom = Vec::new();
        if let Some(orchestrator) = &self.orchestrator_session_id {
            whom.push(format!("your orchestrator {orchestrator}"));
        }
        if let Some(main) = &self.main_session_id {
            whom.push(format!("the project's main session {main}"));
        }
        whom.push("the sender of a message you received (reply_to)".to_string());
        write!(f, "you may address {}", whom.join(", "))
    }
}

/// Why a `PostSessionMessage` was refused. Each says who the caller may address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRefusal {
    /// The caller named itself.
    Oneself { addressable: Addressable },
    /// `to_session_id` is not a session the caller may address: of another project, of another OS
    /// user, unknown here, or one the rule does not reach (a sibling).
    NotAddressable {
        to_session_id: String,
        addressable: Addressable,
    },
    /// `reply_to` names a message the caller never received.
    NotReceived { message_id: String },
}

impl std::fmt::Display for MessageRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRefusal::Oneself { addressable } => {
                write!(f, "a session cannot message itself: {addressable}")
            }
            MessageRefusal::NotAddressable {
                to_session_id,
                addressable,
            } => write!(
                f,
                "session {to_session_id} is not a session you may message: {addressable}"
            ),
            MessageRefusal::NotReceived { message_id } => write!(
                f,
                "you received no message {message_id}: reply_to must name a message addressed to you (ReadSessionMessages, or your message watch)"
            ),
        }
    }
}

/// What the tools need from the daemon: this host's inboxes and its addressing rule, for the caller.
#[async_trait]
pub trait SessionMessagePort: Send + Sync {
    /// Record `request`'s message from `caller`, answered as `{ message_id, to_session_id,
    /// sent_at_unix_ms }`. An error is a [`MessageRefusal`]'s text, or says why nothing was sent.
    async fn post_message(
        &self,
        caller: &HomeCaller,
        request: PostMessageRequest,
    ) -> Result<Value, String>;

    /// `caller`'s unread messages, acknowledged as they are answered: `{ messages, lost, has_more }`.
    async fn read_messages(
        &self,
        caller: &HomeCaller,
        request: ReadMessagesRequest,
    ) -> Result<Value, String>;
}

/// The two tools' definitions, offered to every session role.
pub fn session_message_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            POST_SESSION_MESSAGE,
            "Send another session of this project a message it reads when it next looks: nothing is typed into its terminal. You may message your orchestrator (the session that started you), the project's main session, and the sender of a message you received (pass reply_to); an orchestrator may message any session it can see. Same project and OS user, on this host or another connected host: a recipient on another host is found by itself (your orchestrator, the sender you reply to, or by asking the hosts), or named with daemon_instance_id. A message to another host is queued if its link is down and delivered once it is back. Answers message_id, to_session_id and sent_at_unix_ms. To make a session act now, an orchestrator uses SendSessionMessage instead.",
            json!({
                "type": "object",
                "properties": {
                    "to_session_id": {
                        "type": "string",
                        "description": "The session to message."
                    },
                    "text": {
                        "type": "string",
                        "minLength": 1,
                        "description": "The message: at most 8 KiB."
                    },
                    "reply_to": {
                        "type": "string",
                        "description": "The message_id of a message you received that this one answers."
                    },
                    "daemon_instance_id": {
                        "type": "string",
                        "description": "The host the recipient runs on, as ListProjectSessions reports it; omit to have it found."
                    }
                },
                "required": ["to_session_id", "text"],
                "additionalProperties": false
            }),
        ),
        tool_def(
            READ_SESSION_MESSAGES,
            "Read the messages other sessions sent you that you have not read yet, oldest first, each once: their message_id, from_session_id, from_daemon_instance_id (the sender's host), from, reply_to and text. What is answered counts as read. Under Claude Code, run `tddy-tools monitor --kinds message --ttl 86400` under the Monitor tool to have them streamed instead, as they arrive.",
            json!({
                "type": "object",
                "properties": {
                    "after_message_id": {
                        "type": "string",
                        "description": "Read the messages after this one, read or not; omit for the unread ones."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "description": "At most this many (default and cap 50)."
                    }
                },
                "additionalProperties": false
            }),
        ),
    ]
}

/// The most messages one read asks for, as the tool's schema caps it —
/// `tddy_session_activity::session_messages::MAX_MESSAGES_PER_READ`, which sits above this crate.
pub const MAX_MESSAGES_PER_READ: usize = 50;

/// Run one of the two message tools for `caller`. Bad arguments and the port's refusals come back
/// as an `is_error` outcome saying what to do.
pub async fn execute_session_message_tool(
    port: &dyn SessionMessagePort,
    caller: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    let args: Value = match serde_json::from_str(args_json) {
        Ok(args) => args,
        Err(e) => return ToolOutcome::err(format!("{tool_name}: invalid args_json: {e}")),
    };
    let answered = match tool_name {
        POST_SESSION_MESSAGE => match post_request(&args) {
            Ok(request) => port.post_message(caller, request).await,
            Err(message) => Err(message),
        },
        READ_SESSION_MESSAGES => match read_request(&args) {
            Ok(request) => port.read_messages(caller, request).await,
            Err(message) => Err(message),
        },
        other => Err(format!("unknown session message tool: {other}")),
    };
    match answered {
        Ok(value) => ToolOutcome::ok(value.to_string()),
        Err(message) => ToolOutcome::err(format!("{tool_name}: {message}")),
    }
}

/// What `PostSessionMessage` asks for: a recipient and a text, and the message it replies to when
/// named.
fn post_request(args: &Value) -> Result<PostMessageRequest, String> {
    let to_session_id = match args.get("to_session_id").and_then(Value::as_str) {
        Some(to) if !to.is_empty() => to.to_string(),
        _ => {
            return Err(
                "to_session_id is required: the session to message (ListProjectSessions)"
                    .to_string(),
            )
        }
    };
    let text = match args.get("text").and_then(Value::as_str) {
        Some(text) => text.to_string(),
        None => return Err("text is required: the message to send".to_string()),
    };
    let reply_to = match args.get("reply_to") {
        None | Some(Value::Null) => None,
        Some(Value::String(reply_to)) if !reply_to.is_empty() => Some(reply_to.clone()),
        Some(other) => return Err(format!("reply_to must name a message, not {other}")),
    };
    let daemon_instance_id = match args.get("daemon_instance_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(host)) if host.trim().is_empty() => None,
        Some(Value::String(host)) => Some(host.trim().to_string()),
        Some(other) => return Err(format!("daemon_instance_id must name a host, not {other}")),
    };
    Ok(PostMessageRequest {
        to_session_id,
        text,
        reply_to,
        daemon_instance_id,
    })
}

/// What `ReadSessionMessages` asks for: where to read from, and how much — a `limit` above
/// [`MAX_MESSAGES_PER_READ`] is refused before the port is asked.
fn read_request(args: &Value) -> Result<ReadMessagesRequest, String> {
    let after_message_id = match args.get("after_message_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(after)) if !after.is_empty() => Some(after.clone()),
        Some(other) => return Err(format!("after_message_id must name a message, not {other}")),
    };
    let limit = match args.get("limit") {
        None | Some(Value::Null) => None,
        Some(limit) => {
            let Some(limit) = limit.as_u64() else {
                return Err(format!(
                    "limit must be a whole number of messages, not {limit}"
                ));
            };
            if limit > MAX_MESSAGES_PER_READ as u64 {
                return Err(format!(
                    "limit must be at most {MAX_MESSAGES_PER_READ}: read at most \
                     {MAX_MESSAGES_PER_READ} messages in one call"
                ));
            }
            Some(limit as usize)
        }
    };
    Ok(ReadMessagesRequest {
        after_message_id,
        limit,
    })
}
