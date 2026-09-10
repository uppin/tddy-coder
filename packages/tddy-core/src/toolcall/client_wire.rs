//! The client half of the toolcall wire: what `tddy-tools`' CLI subcommands send over
//! `TDDY_SOCKET`, and what they parse back.
//!
//! Moved here from `tddy-tools`' `cli.rs` by `#unbundle` node 5. Every request shape below has a
//! server-side counterpart in [`super`] — [`super::SubmitRequestWire`], [`super::AskRequestWire`],
//! [`super::TransitionRequestWire`], [`super::ListActionsRequestWire`],
//! [`super::InvokeActionRequestWire`] — which is the whole argument for the two halves living in
//! one crate: a field added to one and not the other is now a diff in one file tree, not a
//! cross-crate drift nothing checks.
//!
//! The response shapes have no server-side struct at all: [`super::ToolCallResponse::to_json`]
//! builds them as `serde_json::json!` literals, so these `Deserialize` structs are the only
//! written-down description of what a caller may rely on.

use serde::{Deserialize, Serialize};

pub use crate::backend::QuestionOption;

/// Wire format for submit request (sent to socket).
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitRequest {
    pub r#type: String,
    pub goal: String,
    pub data: serde_json::Value,
}

/// Wire format for submit response (from socket).
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitResponse {
    pub status: String,
    pub goal: Option<String>,
    pub errors: Option<Vec<String>>,
    /// Transport / relay failures from tddy-coder (`ToolCallResponse::Error`).
    #[serde(default)]
    pub message: Option<String>,
}

/// Wire format for ask request (matches ClarificationQuestion).
#[derive(Debug, Serialize, Deserialize)]
pub struct AskRequest {
    pub r#type: String,
    pub questions: Vec<AskQuestionItem>,
}

/// One question in an [`AskRequest`].
///
/// Deliberately not [`crate::ClarificationQuestion`], which the listener deserializes these into:
/// that type serializes `multi_select` under its own name and carries an `allow_other` field, so
/// substituting it would change the bytes on the wire. What the two *do* share is
/// [`QuestionOption`], re-exported above rather than declared a second time here.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AskQuestionItem {
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
    #[serde(default, rename = "multiSelect")]
    pub multi_select: bool,
}

/// Wire format for ask response.
#[derive(Debug, Serialize, Deserialize)]
pub struct AskResponse {
    pub status: String,
    pub answers: Option<String>,
    pub error: Option<String>,
}

/// Wire format for transition request (sent to socket).
#[derive(Debug, Serialize, Deserialize)]
pub struct TransitionRequest {
    pub r#type: String,
    pub to: String,
    pub provisional: bool,
}

/// Wire format for `list-actions` relay request (sent to TDDY_SOCKET).
#[derive(Debug, Serialize)]
pub struct ListActionsRelayRequest {
    pub r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// Wire format for `list-actions` relay response.
#[derive(Debug, Deserialize)]
pub struct ListActionsRelayResponse {
    pub status: String,
    #[serde(default)]
    pub actions: Option<serde_json::Value>,
    #[serde(default)]
    pub total: Option<usize>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Wire format for `invoke-action` relay request.
#[derive(Debug, Serialize)]
pub struct InvokeActionRelayRequest {
    pub r#type: &'static str,
    pub action: String,
    pub data: String,
}

/// Wire format for `invoke-action` relay response.
#[derive(Debug, Deserialize)]
pub struct InvokeActionRelayResponse {
    pub status: String,
    #[serde(default)]
    pub record: Option<serde_json::Value>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The client's ask request and the listener's `AskRequestWire` describe the same bytes: a
    /// question serialized here must deserialize there, options and `multiSelect` included. The
    /// two halves were in different crates until `#unbundle` node 5, with nothing checking that.
    #[test]
    fn an_ask_request_deserializes_as_the_listener_reads_it() {
        // Given
        let request = AskRequest {
            r#type: "ask".to_string(),
            questions: vec![AskQuestionItem {
                header: "Backend".to_string(),
                question: "Which coding backend?".to_string(),
                options: vec![QuestionOption {
                    label: "claude".to_string(),
                    description: "Claude Code".to_string(),
                }],
                multi_select: true,
            }],
        };

        // When
        let wire: super::super::AskRequestWire =
            serde_json::from_value(serde_json::to_value(&request).expect("serialize"))
                .expect("the listener must read what the client writes");

        // Then
        assert_eq!(wire.r#type, "ask");
        assert_eq!(wire.questions[0].header, "Backend");
        assert_eq!(wire.questions[0].options[0].label, "claude");
        assert!(wire.questions[0].multi_select);
    }

    /// An option sent without a `description` is a label-only option, not a parse failure —
    /// agents routinely omit it.
    #[test]
    fn an_option_without_a_description_reads_as_label_only() {
        // When
        let item: AskQuestionItem = serde_json::from_value(json!({
            "header": "Backend",
            "question": "Which coding backend?",
            "options": [{"label": "claude"}]
        }))
        .expect("a label-only option must parse");

        // Then
        assert_eq!(item.options[0].description, "");
        assert!(!item.multi_select);
    }

    /// `list-actions` omits every unset filter rather than sending nulls, so the listener's
    /// `Option` fields stay `None` instead of being handed an explicit null.
    #[test]
    fn a_list_actions_request_omits_the_filters_it_was_not_given() {
        // Given
        let request = ListActionsRelayRequest {
            r#type: "list-actions",
            path_prefix: None,
            query: Some("build".to_string()),
            limit: None,
            offset: None,
        };

        // When
        let sent = serde_json::to_value(&request).expect("serialize");

        // Then
        assert_eq!(sent, json!({"type": "list-actions", "query": "build"}));
    }
}
