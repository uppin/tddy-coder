//! LSP JSON-RPC client. Speaks to a running server through `tddy-task` channels: requests
//! go out via the task's stdin sender, responses/notifications arrive on the stdout
//! broadcast. Requests are correlated to responses by id; `publishDiagnostics`
//! notifications are cached per document URI.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::LspError;
use crate::protocol::{encode_message, FrameReader};

/// How long a single request waits for its correlated response before giving up, unless a
/// caller raises it with [`LspClient::set_request_timeout`].
///
/// Ten seconds is right for the interactive queries this client was built for and far too
/// short for a code-action request against a cold index, which is why it is a default rather
/// than a cap.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A zero-based line/character position (LSP semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    /// A position at `line`/`character`.
    pub fn at(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A half-open range between two positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// A location: a document URI plus a range within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// A diagnostic (error/warning/…) at a range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    /// LSP severity: 1=Error, 2=Warning, 3=Information, 4=Hint.
    pub severity: u8,
    pub message: String,
    pub source: Option<String>,
}

/// A symbol reported by document/workspace symbol requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub name: String,
    /// LSP `SymbolKind` numeric code.
    pub kind: u8,
    pub location: Location,
    pub container: Option<String>,
}

/// A JSON-RPC `error` object the server sent in place of a `result`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseError {
    /// The server's own error code (e.g. -32801 `ContentModified`).
    pub code: i64,
    pub message: String,
}

/// What a pending request is handed: the response `result`, or the server's error.
type Response = std::result::Result<Value, ResponseError>;

/// Pending in-flight requests, keyed by JSON-RPC id.
type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Response>>>>;
/// Cache of the most recent `publishDiagnostics` per document URI.
type DiagnosticsCache = Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>;

/// Server notifications kept for a consumer to drain, newest last.
type Notifications = Arc<Mutex<VecDeque<Value>>>;

/// How many undrained notifications to keep before dropping the oldest.
///
/// A consumer that never drains must not grow this without bound, and one that drains on a poll
/// only ever needs the recent few — rust-analyzer emits `$/progress` continuously while it loads.
const NOTIFICATION_BACKLOG: usize = 256;

/// A live LSP client attached to one running server.
pub struct LspClient {
    /// Outbound byte stream to the server's stdin (framed JSON-RPC).
    stdin: mpsc::UnboundedSender<Bytes>,
    /// Monotonic source of request ids.
    next_id: AtomicI64,
    /// In-flight requests awaiting their response.
    pending: Pending,
    /// Latest published diagnostics per document.
    diagnostics: DiagnosticsCache,
    /// The background reader draining the server's stdout; aborted on drop.
    reader: JoinHandle<()>,
    /// How long one request waits for its response, in milliseconds. Adjustable through
    /// `&self` because callers only ever hold the client behind an `Arc` from the registry.
    request_timeout_ms: AtomicU64,
    /// The server's `initialize` result, kept because what the server negotiated — its position
    /// encoding above all — decides whether a caller's coordinates mean what it thinks.
    handshake: Mutex<Value>,
    /// Server notifications this client does not consume itself, kept for a caller to drain.
    ///
    /// `$/progress` and `experimental/serverStatus` are the two that matter: they are the only
    /// account of what a server is doing during a load that answers no requests, and dropping
    /// them left every such wait silent and every timeout unable to say where the server got to.
    notifications: Notifications,
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl LspClient {
    /// Attach to a spawned server's channels and complete the
    /// `initialize` / `initialized` handshake against `root_uri`.
    ///
    /// `capabilities` is advertised verbatim. It is the caller's to choose because a server
    /// tailors its answers to it — rust-analyzer returns no code actions whatsoever to a client
    /// that advertised no `codeAction` support, and that is indistinguishable from a range which
    /// supports no refactoring.
    pub async fn initialize(
        stdin: mpsc::UnboundedSender<Bytes>,
        stdout: broadcast::Receiver<Bytes>,
        root_uri: &str,
        capabilities: Value,
        initialization_options: Value,
    ) -> Result<Self, LspError> {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(Mutex::new(VecDeque::new()));

        let reader = tokio::spawn(read_loop(
            stdout,
            Arc::clone(&pending),
            Arc::clone(&diagnostics),
            Arc::clone(&notifications),
            stdin.clone(),
        ));

        let client = Self {
            stdin,
            next_id: AtomicI64::new(1),
            pending,
            diagnostics,
            reader,
            request_timeout_ms: AtomicU64::new(DEFAULT_REQUEST_TIMEOUT.as_millis() as u64),
            handshake: Mutex::new(Value::Null),
            notifications,
        };

        let params = json!({
            "processId": Value::Null,
            "rootUri": root_uri,
            "capabilities": capabilities,
            "initializationOptions": initialization_options,
        });
        let handshake = client.request("initialize", params).await?;
        *client.handshake.lock().unwrap() = handshake;
        client.notify("initialized", json!({}))?;

        Ok(client)
    }

    /// Open a source file as an LSP document so the server indexes it.
    pub async fn did_open(&self, uri: &str, language_id: &str, text: &str) -> Result<(), LspError> {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text,
                }
            }),
        )
    }

    /// Diagnostics for a document (cached `publishDiagnostics` or a pull request).
    pub async fn diagnostics(&self, uri: &str) -> Result<Vec<Diagnostic>, LspError> {
        if let Some(cached) = self.diagnostics.lock().unwrap().get(uri).cloned() {
            return Ok(cached);
        }
        // Nothing published yet — fall back to a pull diagnostic request.
        let result = self
            .request(
                "textDocument/diagnostic",
                json!({ "textDocument": { "uri": uri } }),
            )
            .await?;
        Ok(parse_diagnostics(result.get("items")))
    }

    /// Pull workspace-wide diagnostics, returning one `(uri, diagnostics)` group per
    /// reported document.
    pub async fn workspace_diagnostics(&self) -> Result<Vec<(String, Vec<Diagnostic>)>, LspError> {
        let result = self
            .request("workspace/diagnostic", json!({ "previousResultIds": [] }))
            .await?;
        let mut groups = Vec::new();
        if let Some(items) = result.get("items").and_then(Value::as_array) {
            for item in items {
                let uri = item
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                groups.push((uri, parse_diagnostics(item.get("items"))));
            }
        }
        Ok(groups)
    }

    /// Go-to-definition at a position.
    pub async fn definition(&self, uri: &str, pos: Position) -> Result<Vec<Location>, LspError> {
        let result = self
            .request("textDocument/definition", position_params(uri, pos))
            .await?;
        Ok(parse_locations(&result))
    }

    /// Find-references at a position.
    pub async fn references(&self, uri: &str, pos: Position) -> Result<Vec<Location>, LspError> {
        let mut params = position_params(uri, pos);
        params["context"] = json!({ "includeDeclaration": true });
        let result = self.request("textDocument/references", params).await?;
        Ok(parse_locations(&result))
    }

    /// Hover markdown at a position, if any.
    pub async fn hover(&self, uri: &str, pos: Position) -> Result<Option<String>, LspError> {
        let result = self
            .request("textDocument/hover", position_params(uri, pos))
            .await?;
        Ok(result
            .pointer("/contents/value")
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    /// Document symbols for a file.
    pub async fn symbols(&self, uri: &str) -> Result<Vec<SymbolInfo>, LspError> {
        let result = self
            .request(
                "textDocument/documentSymbol",
                json!({ "textDocument": { "uri": uri } }),
            )
            .await?;
        let symbols = result
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| SymbolInfo {
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        kind: item.get("kind").and_then(Value::as_u64).unwrap_or(0) as u8,
                        location: Location {
                            uri: uri.to_string(),
                            range: parse_range(item.get("range")),
                        },
                        container: item
                            .get("containerName")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(symbols)
    }

    /// Workspace symbol search.
    pub async fn workspace_symbols(&self, query: &str) -> Result<Vec<SymbolInfo>, LspError> {
        let result = self
            .request("workspace/symbol", json!({ "query": query }))
            .await?;
        let symbols = result
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| SymbolInfo {
                        name: item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        kind: item.get("kind").and_then(Value::as_u64).unwrap_or(0) as u8,
                        location: parse_location(item.get("location").unwrap_or(&Value::Null)),
                        container: item
                            .get("containerName")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(symbols)
    }

    /// Graceful `shutdown` / `exit`.
    pub async fn shutdown(&self) -> Result<(), LspError> {
        self.request("shutdown", Value::Null).await?;
        self.notify("exit", Value::Null)?;
        Ok(())
    }

    /// Send a request and await its correlated response `result`.
    pub async fn request_raw(&self, method: &str, params: Value) -> Result<Value, LspError> {
        self.request(method, params).await
    }

    /// Send a notification (no id, no response expected).
    pub async fn notify_raw(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.notify(method, params)
    }

    /// Take every server notification received since the last drain, oldest first.
    pub fn drain_notifications(&self) -> Vec<Value> {
        self.notifications.lock().unwrap().drain(..).collect()
    }

    /// The server's `initialize` result, as it answered the handshake.
    pub fn handshake(&self) -> Value {
        self.handshake.lock().unwrap().clone()
    }

    /// Set how long a single request waits for its response.
    ///
    /// Callers whose own budget governs the wait — `tddy-tools restructure` and its
    /// `--indexing-budget` — raise this so a slow index reports as a slow index rather than
    /// as a request that failed.
    pub fn set_request_timeout(&self, timeout: Duration) {
        self.request_timeout_ms
            .store(timeout.as_millis() as u64, Ordering::SeqCst);
    }

    /// How long a single request currently waits for its response.
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms.load(Ordering::SeqCst))
    }

    /// Send a request and await its correlated response `result`.
    async fn request(&self, method: &str, params: Value) -> Result<Value, LspError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);

        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if self
            .stdin
            .send(Bytes::from(encode_message(&message)))
            .is_err()
        {
            self.pending.lock().unwrap().remove(&id);
            return Err(LspError::ServerExited);
        }

        match tokio::time::timeout(self.request_timeout(), rx).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(error))) => Err(LspError::Server {
                code: error.code,
                message: error.message,
            }),
            Ok(Err(_)) => {
                self.pending.lock().unwrap().remove(&id);
                Err(LspError::ServerExited)
            }
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(LspError::Timeout)
            }
        }
    }

    /// Send a notification (no id, no response expected).
    fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.stdin
            .send(Bytes::from(encode_message(&message)))
            .map_err(|_| LspError::ServerExited)
    }
}

/// Drain the server's stdout: correlate responses to pending requests and cache
/// `publishDiagnostics` notifications.
async fn read_loop(
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
        let mut kept = notifications.lock().unwrap();
        if kept.len() == NOTIFICATION_BACKLOG {
            kept.pop_front();
        }
        kept.push_back(message.clone());
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

/// Standard `{ textDocument, position }` request params.
fn position_params(uri: &str, pos: Position) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": pos.line, "character": pos.character },
    })
}

fn parse_position(value: Option<&Value>) -> Position {
    let value = value.unwrap_or(&Value::Null);
    Position::at(
        value.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
        value.get("character").and_then(Value::as_u64).unwrap_or(0) as u32,
    )
}

fn parse_range(value: Option<&Value>) -> Range {
    let value = value.unwrap_or(&Value::Null);
    Range {
        start: parse_position(value.get("start")),
        end: parse_position(value.get("end")),
    }
}

fn parse_location(value: &Value) -> Location {
    Location {
        uri: value
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        range: parse_range(value.get("range")),
    }
}

fn parse_locations(result: &Value) -> Vec<Location> {
    result
        .as_array()
        .map(|items| items.iter().map(parse_location).collect())
        .unwrap_or_default()
}

fn parse_diagnostics(value: Option<&Value>) -> Vec<Diagnostic> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| Diagnostic {
                    range: parse_range(item.get("range")),
                    severity: item.get("severity").and_then(Value::as_u64).unwrap_or(0) as u8,
                    message: item
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    source: item
                        .get("source")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// During a load that answers no requests, `$/progress` is the only account of what the
    /// server is doing. Dropping it left the wait silent and left a timeout unable to say where
    /// the server had got to.
    #[test]
    fn keeps_a_progress_notification_for_a_caller_to_drain() {
        // Given a client with nothing drained yet
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(Mutex::new(VecDeque::new()));
        let (stdin, _stdin_rx) = mpsc::unbounded_channel();

        // When the server reports progress
        let progress = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": "rustAnalyzer/Roots Scanned", "value": { "kind": "begin" } },
        });
        dispatch(&progress, &pending, &diagnostics, &notifications, &stdin);

        // Then it is retained verbatim
        let kept: Vec<Value> = notifications.lock().unwrap().iter().cloned().collect();
        assert_eq!(kept, vec![progress]);
    }

    /// Diagnostics are consumed into their own cache, so retaining them again would hand every
    /// drainer a stream of notifications it has no use for.
    #[test]
    fn does_not_retain_a_notification_it_consumes_itself() {
        // Given a client with nothing drained yet
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(Mutex::new(VecDeque::new()));
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
        assert!(notifications.lock().unwrap().is_empty());
    }

    /// rust-analyzer emits progress continuously, so a consumer that never drains must not be
    /// able to grow this without bound.
    #[test]
    fn drops_the_oldest_notification_once_the_backlog_is_full() {
        // Given a backlog filled to capacity
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));
        let notifications: Notifications = Arc::new(Mutex::new(VecDeque::new()));
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
        let kept = notifications.lock().unwrap();
        assert_eq!(kept.len(), NOTIFICATION_BACKLOG);
        assert_eq!(kept.front().unwrap().pointer("/params/n"), Some(&json!(1)));
        assert_eq!(
            kept.back().unwrap().pointer("/params/n"),
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
        let notifications: Notifications = Arc::new(Mutex::new(VecDeque::new()));
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
        let notifications: Notifications = Arc::new(Mutex::new(VecDeque::new()));
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
