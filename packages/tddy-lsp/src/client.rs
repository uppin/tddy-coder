//! LSP JSON-RPC client. Speaks to a running server through `tddy-task` channels: requests
//! go out via the task's stdin sender, responses/notifications arrive on the stdout
//! broadcast. Requests are correlated to responses by id; `publishDiagnostics`
//! notifications are cached per document URI.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::LspError;
use crate::protocol::{encode_message, FrameReader};

/// How long a single request waits for its correlated response before giving up.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Pending in-flight requests, keyed by JSON-RPC id.
type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>;
/// Cache of the most recent `publishDiagnostics` per document URI.
type DiagnosticsCache = Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>;

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
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl LspClient {
    /// Attach to a spawned server's channels and complete the
    /// `initialize` / `initialized` handshake against `root_uri`.
    pub async fn initialize(
        stdin: mpsc::UnboundedSender<Bytes>,
        stdout: broadcast::Receiver<Bytes>,
        root_uri: &str,
    ) -> Result<Self, LspError> {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: DiagnosticsCache = Arc::new(Mutex::new(HashMap::new()));

        let reader = tokio::spawn(read_loop(
            stdout,
            Arc::clone(&pending),
            Arc::clone(&diagnostics),
        ));

        let client = Self {
            stdin,
            next_id: AtomicI64::new(1),
            pending,
            diagnostics,
            reader,
        };

        let params = json!({
            "processId": Value::Null,
            "rootUri": root_uri,
            "capabilities": {},
        });
        client.request("initialize", params).await?;
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

        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => Ok(result),
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
) {
    let mut frames = FrameReader::new();
    loop {
        match stdout.recv().await {
            Ok(bytes) => {
                frames.push(&bytes);
                while let Some(message) = frames.next_message() {
                    dispatch(&message, &pending, &diagnostics);
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

/// Route one incoming message to a pending request or the diagnostics cache.
fn dispatch(message: &Value, pending: &Pending, diagnostics: &DiagnosticsCache) {
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
        }
        return;
    }

    // Otherwise it is a response to one of our requests.
    if let Some(id) = message.get("id").and_then(Value::as_i64) {
        if let Some(tx) = pending.lock().unwrap().remove(&id) {
            let result = message.get("result").cloned().unwrap_or(Value::Null);
            let _ = tx.send(result);
        }
    }
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
