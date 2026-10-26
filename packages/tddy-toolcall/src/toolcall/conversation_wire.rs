//! Wire formats of the conversation-spawn verbs (`spawn-conversation`, and the universal start's
//! `start-conversation`, `conversation-start-status` and `conversation-start-cancel`).

use serde::Deserialize;

/// Wire format for `spawn-conversation` request (from tddy-tools, generic managed session).
///
/// The generic sibling of [`SpawnChildRequestWire`](super::SpawnChildRequestWire): instead of resolving a planned PR-stack node
/// id, it carries a free-form `prompt` for a brand-new interactive conversation, plus an optional
/// `branch` name and `base_ref` to root the new worktree on.
#[derive(Debug, Deserialize)]
pub struct SpawnConversationRequestWire {
    pub r#type: String,
    /// Prompt to seed the new interactive conversation with.
    pub prompt: String,
    /// Optional branch name for the new worktree; derived from the prompt when absent.
    #[serde(default)]
    pub branch: Option<String>,
    /// Optional base ref to root the new worktree on; defaults to the session's base when absent.
    #[serde(default)]
    pub base_ref: Option<String>,
}

/// Wire format for `start-conversation` request (from tddy-tools): `StartSession`'s own arguments,
/// as the `StartSessionArgs` JSON object, for the daemon to start as the calling session's
/// conversation.
#[derive(Debug, Deserialize)]
pub struct StartConversationRequestWire {
    pub r#type: String,
    /// `tddy_tool_engine::start_session_args::StartSessionArgs`, left as JSON: this crate does not
    /// depend on the tool engine, and the handler that starts the session parses it.
    pub args: serde_json::Value,
}

/// Wire format for `conversation-start-status` and `conversation-start-cancel` requests (from
/// tddy-tools): the `start_id` the start's handle named.
#[derive(Debug, Deserialize)]
pub struct ConversationStartRequestWire {
    pub r#type: String,
    pub start_id: String,
}
