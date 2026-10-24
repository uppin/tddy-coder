//! A coding subagent's permission request, yielded to the caller, and the caller's answer.
//!
//! A coding agent asks before it edits or runs something its policy does not pre-approve. That
//! question is the invoking agent's to answer: the turn ends with
//! [`super::StopReason::PermissionRequested`] and the request, and `subagent_resume { permission }`
//! delivers the answer to the same, still-running agent turn.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-permission-yield.md

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
