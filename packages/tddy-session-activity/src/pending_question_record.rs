//! The pending question a claude-cli hook report records (`#home-inbox` 3/8).
//!
//! A `PermissionRequest`, or a `PreToolUse` of `AskUserQuestion`, names a tool and that tool's
//! input. This module turns that input into the [`PendingQuestion`] the daemon keeps: the flat
//! fields `#start-parity` 6/9 reads, and the structured ones this node adds. Pure, so the mapping is
//! tested without a session directory.

use tddy_core::pending_question::{PendingQuestion, PendingQuestionItem, PendingQuestionOption};
use tddy_core::ASK_USER_QUESTION_TOOL;
use tddy_rpc::Status;

/// The record a hook report that asks something leaves: the tool and its input as the hook sent
/// them, a fresh `request_id`, and the question in the flat fields.
///
/// For an `AskUserQuestion` the flat fields mirror the first question and `allow_other` is true. For
/// a permission the question reads `Run <tool>: <summary>`.
///
/// # Errors
///
/// An `AskUserQuestion` whose input is not JSON, or names no question, is refused as an invalid
/// request: a question with nothing to ask would be an operator alert with nothing in it.
pub(crate) fn pending_question_for_hook(
    tool_name: &str,
    tool_input_json: &str,
    request_id: String,
    at_unix_ms: u64,
) -> Result<PendingQuestion, Status> {
    let mut record = PendingQuestion {
        request_id,
        tool_name: tool_name.to_string(),
        tool_input_json: tool_input_json.to_string(),
        at_unix_ms,
        ..Default::default()
    };
    if tool_name == ASK_USER_QUESTION_TOOL {
        record.questions = questions_of(tool_input_json)?;
        let first = record
            .questions
            .first()
            .ok_or_else(|| Status::invalid_argument("AskUserQuestion names no question"))?;
        record.kind = if first.multi_select {
            "multi_select"
        } else {
            "select"
        }
        .to_string();
        record.header = first.header.clone();
        record.question = first.question.clone();
        record.options = first
            .options
            .iter()
            .map(|option| option.label.clone())
            .collect();
        record.allow_other = true;
    } else {
        record.kind = "permission".to_string();
        let summary = record.summary();
        record.question = if summary.is_empty() {
            format!("Run {tool_name}")
        } else {
            format!("Run {tool_name}: {summary}")
        };
    }
    Ok(record)
}

/// Every question of an `AskUserQuestion` input, in the order it asks them.
fn questions_of(tool_input_json: &str) -> Result<Vec<PendingQuestionItem>, Status> {
    let input: serde_json::Value = serde_json::from_str(tool_input_json).map_err(|e| {
        Status::invalid_argument(format!("AskUserQuestion input is not its JSON: {e}"))
    })?;
    let Some(questions) = input.get("questions").and_then(serde_json::Value::as_array) else {
        return Ok(Vec::new());
    };
    Ok(questions.iter().map(question_of).collect())
}

/// One question of the call: its header, its text, its options and whether several may be chosen.
fn question_of(asked: &serde_json::Value) -> PendingQuestionItem {
    let options = asked
        .get("options")
        .and_then(serde_json::Value::as_array)
        .map(|options| {
            options
                .iter()
                .map(|option| PendingQuestionOption {
                    label: text_field(option, "label"),
                    description: text_field(option, "description"),
                })
                .collect()
        })
        .unwrap_or_default();
    PendingQuestionItem {
        header: text_field(asked, "header"),
        question: text_field(asked, "question"),
        options,
        multi_select: asked
            .get("multiSelect")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_default(),
    }
}

/// The string under `key`, or empty when it is absent or not a string.
fn text_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A permission for `cargo publish`, recorded under a fixed id.
    #[test]
    fn a_permission_is_a_permission_asking_to_run_its_command() {
        // When
        let record = pending_question_for_hook(
            "Bash",
            r#"{"command":"cargo publish"}"#,
            "request-1".to_string(),
            7,
        )
        .expect("a permission is recorded");

        // Then
        assert_eq!(record.kind, "permission");
        assert_eq!(record.question, "Run Bash: cargo publish");
        assert_eq!(record.request_id, "request-1");
        assert!(!record.allow_other);
    }

    /// A question whose input is not JSON is refused, and nothing is recorded for it.
    #[test]
    fn an_ask_user_question_whose_input_is_not_json_is_refused() {
        // When
        let refusal = pending_question_for_hook(
            ASK_USER_QUESTION_TOOL,
            "not json",
            "request-2".to_string(),
            7,
        )
        .expect_err("malformed input must be refused");

        // Then
        assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    }

    /// A question call that names no question is refused for the same reason.
    #[test]
    fn an_ask_user_question_that_names_no_question_is_refused() {
        // When
        let refusal = pending_question_for_hook(
            ASK_USER_QUESTION_TOOL,
            r#"{"questions":[]}"#,
            "request-3".to_string(),
            7,
        )
        .expect_err("a call with no question must be refused");

        // Then
        assert_eq!(refusal.code(), tddy_rpc::Code::InvalidArgument);
    }
}
