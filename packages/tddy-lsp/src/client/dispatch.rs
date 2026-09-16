//! Reading the server's stdout: one framed message at a time, routed to whatever is waiting for
//! it — a pending request, the diagnostics cache, the notification sink, or a reply the server is
//! blocked on.

use bytes::Bytes;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc};

use super::parse::parse_diagnostics;
use super::{DiagnosticsCache, Notifications, Pending, Response, ResponseError};
use crate::protocol::{encode_message, FrameReader};

/// Drain the server's stdout: correlate responses to pending requests and cache
/// `publishDiagnostics` notifications.
pub(super) async fn read_loop(
    mut stdout: broadcast::Receiver<Bytes>,
    pending: Pending,
    diagnostics: DiagnosticsCache,
    notifications: Notifications,
    stdin: mpsc::UnboundedSender<Bytes>,
) {
    let mut frames = FrameReader::new();
    loop {
        match stdout.recv().await {
            Ok(bytes) => {
                frames.push(&bytes);
                while let Some(message) = frames.next_message() {
                    dispatch(&message, &pending, &diagnostics, &notifications, &stdin);
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Route one incoming message to a pending request, the diagnostics cache, or (for a
/// server→client request) an acknowledgement reply.
fn dispatch(
    message: &Value,
    pending: &Pending,
    diagnostics: &DiagnosticsCache,
    notifications: &Notifications,
    stdin: &mpsc::UnboundedSender<Bytes>,
) {
    // Notifications and server-to-client requests carry a `method`.
    if let Some(method) = message.get("method").and_then(Value::as_str) {
        if method == "textDocument/publishDiagnostics" {
            if let Some(params) = message.get("params") {
                let uri = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                diagnostics
                    .lock()
                    .unwrap()
                    .insert(uri, parse_diagnostics(params.get("diagnostics")));
            }
            return;
        }
        // A `method` with an `id` is a server→client request (e.g. rust-analyzer's
        // `client/registerCapability`, `window/workDoneProgress/create`,
        // `workspace/configuration`). It expects a response — acknowledge it minimally so
        // the server does not stall waiting on us.
        if let Some(id) = message.get("id").cloned() {
            let reply = server_request_reply(method, message.get("params"), id);
            let _ = stdin.send(Bytes::from(encode_message(&reply)));
            return;
        }
        // A notification this client does not consume itself. Kept rather than dropped: during a
        // load that answers no requests, `$/progress` is the only account of what the server is
        // doing, and `experimental/serverStatus` the only signal that it has finished.
        notifications.record(message.clone());
        return;
    }

    // Otherwise it is a response to one of our requests. A JSON-RPC response carries either
    // a `result` or an `error`, never both — reading `result` alone and defaulting to null
    // turned every server error into a successful empty answer.
    if let Some(id) = message.get("id").and_then(Value::as_i64) {
        if let Some(tx) = pending.lock().unwrap().remove(&id) {
            let _ = tx.send(response_payload(message));
        }
    }
}

/// Split a response into its `result` or its `error`.
fn response_payload(message: &Value) -> Response {
    if let Some(error) = message.get("error") {
        return Err(ResponseError {
            code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    }
    Ok(message.get("result").cloned().unwrap_or(Value::Null))
}

/// Build the response to a server→client request. `workspace/configuration` expects an
/// array with one entry per requested item; every other request is acknowledged with a
/// `null` result (sufficient for `registerCapability` / `workDoneProgress/create`).
fn server_request_reply(method: &str, params: Option<&Value>, id: Value) -> Value {
    let result = match method {
        "workspace/configuration" => {
            let count = params
                .and_then(|p| p.get("items"))
                .and_then(Value::as_array)
                .map(|items| items.len())
                .unwrap_or(0);
            Value::Array(vec![Value::Null; count])
        }
        _ => Value::Null,
    };
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

#[cfg(test)]
mod tests {
    use super::super::notifications::{NotificationSink, NOTIFICATION_BACKLOG};
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::oneshot;

    /// During a load that answers no requests, `$/progress` is the only account of what the
    /// server is doing. Dropping it left the wait silent and left a timeout unable to say where
    /// the server had got to.
    #[test]
    fn keeps_a_progress_notification_for_a_caller_to_drain() {
        // Given a client with nothing drained yet
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(NotificationSink::new());
        let (stdin, _stdin_rx) = mpsc::unbounded_channel();

        // When the server reports progress
        let progress = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": "rustAnalyzer/Roots Scanned", "value": { "kind": "begin" } },
        });
        dispatch(&progress, &pending, &diagnostics, &notifications, &stdin);

        // Then it is retained verbatim
        assert_eq!(notifications.drain(), vec![progress]);
    }

    /// Diagnostics are consumed into their own cache, so retaining them again would hand every
    /// drainer a stream of notifications it has no use for.
    #[test]
    fn does_not_retain_a_notification_it_consumes_itself() {
        // Given a client with nothing drained yet
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(NotificationSink::new());
        let (stdin, _stdin_rx) = mpsc::unbounded_channel();

        // When diagnostics are published
        dispatch(
            &json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": { "uri": "file:///a.rs", "diagnostics": [] },
            }),
            &pending,
            &diagnostics,
            &notifications,
            &stdin,
        );

        // Then they land in the diagnostics cache and not in the backlog
        assert!(diagnostics.lock().unwrap().contains_key("file:///a.rs"));
        assert!(notifications.drain().is_empty());
    }

    /// rust-analyzer emits progress continuously, so a consumer that never drains must not be
    /// able to grow this without bound.
    #[test]
    fn drops_the_oldest_notification_once_the_backlog_is_full() {
        // Given a backlog filled to capacity
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(NotificationSink::new());
        let (stdin, _stdin_rx) = mpsc::unbounded_channel();
        for n in 0..NOTIFICATION_BACKLOG {
            dispatch(
                &json!({ "jsonrpc": "2.0", "method": "$/progress", "params": { "n": n } }),
                &pending,
                &diagnostics,
                &notifications,
                &stdin,
            );
        }

        // When one more arrives
        dispatch(
            &json!({ "jsonrpc": "2.0", "method": "$/progress", "params": { "n": "last" } }),
            &pending,
            &diagnostics,
            &notifications,
            &stdin,
        );

        // Then the backlog is still capped, and it is the oldest that went
        let kept = notifications.drain();
        assert_eq!(kept.len(), NOTIFICATION_BACKLOG);
        assert_eq!(kept.first().unwrap().pointer("/params/n"), Some(&json!(1)));
        assert_eq!(
            kept.last().unwrap().pointer("/params/n"),
            Some(&json!("last"))
        );
    }

    #[test]
    fn acknowledges_register_capability_with_a_null_result() {
        // Given a server->client `client/registerCapability` request
        let id = json!(7);

        // When we build the reply
        let reply = server_request_reply("client/registerCapability", None, id);

        // Then it is a well-formed response with a null result
        assert_eq!(reply, json!({ "jsonrpc": "2.0", "id": 7, "result": null }));
    }

    /// A JSON-RPC error response carries no `result`. Reading `result` and defaulting to null
    /// turns every server error into a successful empty answer, which is how a
    /// `ContentModified` — the code rust-analyzer uses to mean "ask again" — was reaching
    /// callers as "the server had nothing to offer".
    #[test]
    fn hands_a_server_error_response_to_the_waiting_request() {
        // Given a request awaiting a response on id 4
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(NotificationSink::new());
        let (stdin, _stdin_rx) = mpsc::unbounded_channel();
        let (tx, rx) = oneshot::channel();
        pending.lock().unwrap().insert(4, tx);

        // When the server answers it with a JSON-RPC error instead of a result
        dispatch(
            &json!({
                "jsonrpc": "2.0",
                "id": 4,
                "error": { "code": -32801, "message": "content modified" },
            }),
            &pending,
            &diagnostics,
            &notifications,
            &stdin,
        );

        // Then the waiting request is handed the error, not an empty success
        let delivered = rx.blocking_recv().expect("a response was delivered");
        let error = delivered.expect_err("a JSON-RPC error must not arrive as a result");
        assert_eq!(
            (error.code, error.message.as_str()),
            (-32801, "content modified")
        );
    }

    #[test]
    fn hands_a_successful_response_to_the_waiting_request() {
        // Given a request awaiting a response on id 5
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(NotificationSink::new());
        let (stdin, _stdin_rx) = mpsc::unbounded_channel();
        let (tx, rx) = oneshot::channel();
        pending.lock().unwrap().insert(5, tx);

        // When the server answers with a result
        dispatch(
            &json!({ "jsonrpc": "2.0", "id": 5, "result": { "ok": true } }),
            &pending,
            &diagnostics,
            &notifications,
            &stdin,
        );

        // Then the result arrives unchanged
        let delivered = rx.blocking_recv().expect("a response was delivered");
        assert_eq!(
            delivered.expect("a successful result"),
            json!({ "ok": true })
        );
    }

    #[test]
    fn answers_workspace_configuration_with_one_null_per_requested_item() {
        // Given a `workspace/configuration` request for three items
        let params = json!({ "items": [{}, {}, {}] });

        // When we build the reply
        let reply = server_request_reply("workspace/configuration", Some(&params), json!(3));

        // Then the result is an array with one null per item
        assert_eq!(
            reply,
            json!({ "jsonrpc": "2.0", "id": 3, "result": [null, null, null] })
        );
    }
}
