//! `workspace/executeCommand`, and the `workspace/applyEdit` requests a server sends while one runs
//! (`#reshape` extension-code-actions, R2–R3).
//!
//! Many servers perform a code action by asking the client to apply an edit from inside the command
//! that implements it. Nothing a client is sent that way is written by this crate: every edit that
//! arrives during the call is captured, answered `{ "applied": true }` so the server finishes its
//! command, and handed back to the caller, whose own apply path (journal, compile gate) writes it.
//! A `workspace/applyEdit` that arrives while no command runs keeps today's answer, `null`; one
//! that carries no `edit` is answered `{ "applied": false }` and not captured.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::{json, Value};

use super::{LspClient, LspError};

/// What a `workspace/executeCommand` came back with.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutedCommand {
    /// The command's own `result`, verbatim — the server's answer to `workspace/executeCommand`.
    ///
    /// No caller in this workspace reads it today (the edits are what a code action produces), but
    /// it is the command's answer and part of this general LSP client's API, so it is kept.
    pub result: Value,
    /// The `edit` of every `workspace/applyEdit` the server sent during the call, a `WorkspaceEdit`
    /// each, in arrival order. Not applied — the caller's to apply.
    pub applied_edits: Vec<Value>,
}

/// The edits a server sent through `workspace/applyEdit` while a command ran: `Some` exactly while
/// [`LspClient::execute_command`] is waiting on its answer, `None` otherwise.
pub(super) type ApplyEditCapture = Arc<Mutex<Option<Vec<Value>>>>;

/// Held for the whole of one [`LspClient::execute_command`], so a second call on the same client
/// waits for the first instead of sharing — and corrupting — its capture.
pub(super) type CommandGate = tokio::sync::Mutex<()>;

/// The failure reason a `workspace/applyEdit` carrying no `edit` is answered with.
const NO_EDIT_FAILURE_REASON: &str = "workspace/applyEdit carried no `edit`";

/// Lock `capture`, recovering it if a holder panicked.
///
/// Recovery is sound here, unlike for this crate's other mutexes: every write under this lock is a
/// single whole-value assignment or one `push`, so a panicking holder cannot leave it half-updated,
/// and [`CommandInFlight`]'s `Drop` must not panic.
fn lock(capture: &ApplyEditCapture) -> MutexGuard<'_, Option<Vec<Value>>> {
    capture.lock().unwrap_or_else(PoisonError::into_inner)
}

impl LspClient {
    /// Send `workspace/executeCommand` for `command` with `arguments`, and wait for its answer.
    ///
    /// Every `workspace/applyEdit` the server sends before that answer arrives is captured — its
    /// `edit`, in arrival order — and answered `{ "applied": true }`; nothing is written. One
    /// command runs at a time per client — a concurrent call waits for this one to finish — so a
    /// capture belongs to exactly one call.
    pub async fn execute_command(
        &self,
        command: &str,
        arguments: Vec<Value>,
    ) -> Result<ExecutedCommand, LspError> {
        let _one_at_a_time = self.command_gate.lock().await;
        let _running = CommandInFlight::begin(&self.apply_edits);
        let result = self
            .request(
                "workspace/executeCommand",
                json!({ "command": command, "arguments": arguments }),
            )
            .await?;
        let applied_edits = lock(&self.apply_edits).take().unwrap_or_default();
        Ok(ExecutedCommand {
            result,
            applied_edits,
        })
    }
}

/// The answer to a server's `workspace/applyEdit` request with `params`: while a command runs
/// (`capture` holds `Some`), the request's `edit` is appended to the capture and the answer is
/// `{ "applied": true }`; otherwise nothing is captured and the answer is `null`, as before. A
/// request with no `edit` is never captured: while a command runs it is answered
/// `{ "applied": false }` with a `failureReason`, so the server learns nothing was applied.
pub(super) fn apply_edit_answer(capture: &ApplyEditCapture, params: Option<&Value>) -> Value {
    let mut captured = lock(capture);
    let Some(edits) = captured.as_mut() else {
        return Value::Null;
    };
    let Some(edit) = params.and_then(|params| params.get("edit")) else {
        return json!({ "applied": false, "failureReason": NO_EDIT_FAILURE_REASON });
    };
    edits.push(edit.clone());
    json!({ "applied": true })
}

/// A command's capture, open for as long as this is held: opened empty by [`Self::begin`], and
/// closed on drop however the call ends — answered, failed, timed out, or its future dropped — so
/// an edit sent after the command is answered `null` and not kept.
struct CommandInFlight<'a>(&'a ApplyEditCapture);

impl<'a> CommandInFlight<'a> {
    fn begin(capture: &'a ApplyEditCapture) -> Self {
        *lock(capture) = Some(Vec::new());
        Self(capture)
    }
}

impl Drop for CommandInFlight<'_> {
    fn drop(&mut self) {
        *lock(self.0) = None;
    }
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
    fn an_apply_edit_with_no_edit_is_answered_not_applied_and_not_kept() {
        // Given a command in flight
        let capture = a_capture_while_a_command_runs();

        // When the server asks to apply a request that carries no edit
        let answer = apply_edit_answer(&capture, Some(&json!({ "label": "fake" })));

        // Then it is told nothing was applied, and nothing is captured
        assert_eq!(
            (answer, capture.lock().unwrap().clone()),
            (
                json!({ "applied": false, "failureReason": NO_EDIT_FAILURE_REASON }),
                Some(Vec::new())
            )
        );
    }

    #[test]
    fn a_poisoned_capture_still_closes_without_panicking() {
        // Given a capture whose lock a panicking holder poisoned
        let capture = a_capture_while_a_command_runs();
        let poisoner = Arc::clone(&capture);
        let _ = std::thread::spawn(move || {
            let _held = poisoner.lock().unwrap();
            panic!("poison the capture");
        })
        .join();

        // When a command's capture is closed over it
        drop(CommandInFlight(&capture));

        // Then the capture is closed
        assert_eq!(*lock(&capture), None);
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
