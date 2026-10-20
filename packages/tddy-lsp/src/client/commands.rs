//! `workspace/executeCommand`, and the `workspace/applyEdit` requests a server sends while one runs
//! (`#reshape` extension-code-actions, R2–R3).
//!
//! Many servers perform a code action by asking the client to apply an edit from inside the command
//! that implements it. Nothing a client is sent that way is written by this crate: every edit that
//! arrives during the call is captured, answered `{ "applied": true }` so the server finishes its
//! command, and handed back to the caller, whose own apply path (journal, compile gate) writes it.
//! A `workspace/applyEdit` that arrives while no command runs keeps today's answer, `null`.

use std::sync::{Arc, Mutex};

use serde_json::Value;

use super::{LspClient, LspError};

/// What a `workspace/executeCommand` came back with.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutedCommand {
    /// The command's own `result`, verbatim.
    pub result: Value,
    /// The `edit` of every `workspace/applyEdit` the server sent during the call, a `WorkspaceEdit`
    /// each, in arrival order. Not applied — the caller's to apply.
    pub applied_edits: Vec<Value>,
}

/// The edits a server sent through `workspace/applyEdit` while a command ran: `Some` exactly while
/// [`LspClient::execute_command`] is waiting on its answer, `None` otherwise.
// TODO(reshape-extension-code-actions): held by the client, set by `execute_command` and read by
// dispatch once implemented.
pub(super) type ApplyEditCapture = Arc<Mutex<Option<Vec<Value>>>>;

impl LspClient {
    /// Send `workspace/executeCommand` for `command` with `arguments`, and wait for its answer.
    ///
    /// Every `workspace/applyEdit` the server sends before that answer arrives is captured — its
    /// `edit`, in arrival order — and answered `{ "applied": true }`; nothing is written. One
    /// command runs at a time per client, so a capture belongs to exactly one call.
    pub async fn execute_command(
        &self,
        command: &str,
        arguments: Vec<Value>,
    ) -> Result<ExecutedCommand, LspError> {
        let _ = (command, arguments);
        // TODO(reshape-extension-code-actions): implement
        todo!("LspClient::execute_command")
    }
}

/// The answer to a server's `workspace/applyEdit` request with `params`: while a command runs
/// (`capture` holds `Some`), the request's `edit` is appended to the capture and the answer is
/// `{ "applied": true }`; otherwise nothing is captured and the answer is `null`, as before.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "TODO(reshape-extension-code-actions): server_request_reply answers from it once implemented"
    )
)]
pub(super) fn apply_edit_answer(capture: &ApplyEditCapture, params: Option<&Value>) -> Value {
    let _ = (capture, params);
    // TODO(reshape-extension-code-actions): implement
    todo!("apply_edit_answer")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn an_apply_edit_of(new_text: &str) -> Value {
        json!({
            "label": "fake",
            "edit": { "changes": { "file:///w/a.toml": [{
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } },
                "newText": new_text,
            }] } },
        })
    }

    fn a_capture_while_a_command_runs() -> ApplyEditCapture {
        Arc::new(Mutex::new(Some(Vec::new())))
    }

    #[test]
    fn an_apply_edit_while_a_command_runs_is_captured_and_answered_applied() {
        // Given a command in flight
        let capture = a_capture_while_a_command_runs();

        // When the server asks for two edits
        let answers = [
            apply_edit_answer(&capture, Some(&an_apply_edit_of("a"))),
            apply_edit_answer(&capture, Some(&an_apply_edit_of("b"))),
        ];

        // Then both are answered applied, and both edits are kept in arrival order
        assert_eq!(
            (answers, capture.lock().unwrap().clone()),
            (
                [json!({ "applied": true }), json!({ "applied": true })],
                Some(vec![
                    an_apply_edit_of("a")["edit"].clone(),
                    an_apply_edit_of("b")["edit"].clone()
                ])
            )
        );
    }

    #[test]
    fn an_apply_edit_while_no_command_runs_is_answered_null_and_not_kept() {
        // Given no command in flight
        let capture: ApplyEditCapture = Arc::new(Mutex::new(None));

        // When the server asks for an edit
        let answer = apply_edit_answer(&capture, Some(&an_apply_edit_of("a")));

        // Then it gets today's answer and nothing is captured
        assert_eq!(
            (answer, capture.lock().unwrap().clone()),
            (Value::Null, None)
        );
    }
}
