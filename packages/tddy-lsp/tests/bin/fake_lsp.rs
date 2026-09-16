//! A deterministic fake LSP server used by the `tddy-lsp` integration tests.
//!
//! It speaks `Content-Length`-framed JSON-RPC on stdin/stdout and answers a fixed set of
//! requests with known values (see the `FAKE_*` constants, which the tests assert
//! against). It is NOT part of the shipped product — only a test double so the tests
//! never need a real rust-analyzer.
//!
//! Modes (via argv):
//! - `--exit-immediately` — exit at startup (simulate a server that crashes on spawn).
//! - `--hang` — ignore `shutdown`/`exit` and keep running (simulate an unresponsive
//!   server, so the task registry's SIGTERM→SIGKILL escalation is exercised).
//! - `--cold-hovers N` — answer the first N `textDocument/hover` requests with `null`, the way a
//!   server that has not finished loading the crate graph does, then answer normally. Lets a test
//!   drive a slow index without waiting on a real one. Each such answer is preceded by a
//!   `$/progress` notification, because a real server that answers nothing reports what it is doing
//!   instead — and a consumer that forwards progress has nothing to forward otherwise.
//! - `--loads-crate-graph` — narrate a crate-graph load and then report `experimental/serverStatus`
//!   with `quiescent: true`, the way a real rust-analyzer does once its graph is queryable. Without
//!   this the fake never claims to load anything, so a consumer whose readiness *is* the server's
//!   quiescence has nothing to wait for. Independent of `--cold-hovers`, and off unless asked for,
//!   so every other mode is byte-identical to what it was.
//!
//! Two methods model failure modes a real rust-analyzer has and the fixed replies above do
//! not: `textDocument/codeAction` answers with a JSON-RPC `ContentModified` error, and
//! `tddy/neverAnswers` sends no response at all.
//!
//! `tddy/initializeParams` replays the `initialize` params the client sent, so a test can assert
//! what the client actually advertised rather than what it meant to.
//!
//! `tddy/documentVersions` replays the document versions the client sent, in arrival order, as
//! `{ uri: [version, …] }` — covering `didOpen`, `didChange` and `didClose` alike. A test asserts
//! on the sequence a server actually received, which is the only place a client's version
//! bookkeeping is observable. Entries are never forgotten, so a close followed by a reopen shows as
//! a restart in the sequence rather than as a gap.
//!
//! `tddy/documentSyncLog` replays the same notifications keyed the same way, but as
//! `{ uri: [{ method, version }, …] }` — so a test can assert *which* notification a caller sent,
//! which `tddy/documentVersions` cannot show: a re-open at 1 and an edit at 1 look alike there.
//!
//! `tddy/documentTexts` replays the latest contents the client announced per URI, as
//! `{ uri: text }`, covering `didOpen`'s `text` and `didChange`'s full-text content change. A test
//! asserts that an edit made between two binds actually reached the server.
//!
//! `tddy/cancelledRequests` replays the methods of the requests the client sent `$/cancelRequest`
//! for, in arrival order. A test asserts that abandoning a request told the server to stop working
//! on it, rather than only that the caller stopped waiting.
//!
//! `tddy/floodStderr` writes [`STDERR_FLOOD_BYTES`] to stderr before replying. A host that pipes
//! the server's stderr without draining it fills the pipe buffer, and the server then blocks
//! mid-write — so both this reply and every later one go unanswered.

use std::io::{Read, Write};

use serde_json::{json, Value};
use tddy_lsp::protocol::{encode_message, FrameReader};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--exit-immediately") {
        std::process::exit(0);
    }
    let hang = args.iter().any(|a| a == "--hang");
    let loads_crate_graph = args.iter().any(|a| a == "--loads-crate-graph");
    let cold_hovers = args
        .iter()
        .position(|a| a == "--cold-hovers")
        .and_then(|at| args.get(at + 1))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);

    let mut reader = FrameReader::new();
    let mut chunk = [0u8; 4096];
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    loop {
        let n = match stdin.read(&mut chunk) {
            Ok(0) => break, // EOF
            Ok(n) => n,
            Err(_) => break,
        };
        reader.push(&chunk[..n]);
        while let Some(message) = reader.next_message() {
            handle_message(&message, hang, cold_hovers, loads_crate_graph);
        }
    }
}

/// The `initialize` params as received, so `tddy/initializeParams` can replay them.
static INITIALIZE_PARAMS: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

/// Every document-sync notification received, in arrival order: method, URI, the version it
/// declared (a `didClose` declares none) and the contents it announced (a `didClose` announces
/// none). One log rather than one store per replay, so the two views cannot disagree.
static DOCUMENT_SYNC_LOG: std::sync::Mutex<Option<Vec<DocumentSyncEntry>>> =
    std::sync::Mutex::new(None);

/// The method each request id was received on, so a later `$/cancelRequest` can name it.
static REQUEST_METHODS: std::sync::Mutex<Option<Vec<(Value, String)>>> =
    std::sync::Mutex::new(None);

/// The methods of the requests the client asked to cancel, in arrival order.
static CANCELLED_REQUESTS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

/// How many `textDocument/hover` requests have been answered with `null` so far.
static COLD_HOVERS_SERVED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Enough stderr to exceed a pipe buffer on every platform this runs on (64 KiB is the usual
/// ceiling), so an undrained pipe blocks rather than merely filling.
const STDERR_FLOOD_BYTES: usize = 512 * 1024;

/// One document-sync notification as received.
#[derive(Clone)]
struct DocumentSyncEntry {
    method: String,
    uri: String,
    version: Option<i64>,
    text: Option<String>,
}

fn record_document_sync(message: &Value, method: &str) {
    DOCUMENT_SYNC_LOG
        .lock()
        .unwrap()
        .get_or_insert_with(Vec::new)
        .push(DocumentSyncEntry {
            method: method.to_string(),
            uri: notified_uri(message),
            version: notified_version(message),
            text: notified_text(message),
        });
}

fn recorded_document_sync() -> Vec<DocumentSyncEntry> {
    DOCUMENT_SYNC_LOG
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
}

/// `{ uri: [version, …] }`, each URI's versions in the order they arrived.
fn recorded_document_versions() -> Value {
    let mut by_uri: std::collections::BTreeMap<String, Vec<i64>> =
        std::collections::BTreeMap::new();
    for entry in recorded_document_sync() {
        if let Some(version) = entry.version {
            by_uri.entry(entry.uri).or_default().push(version);
        }
    }
    json!(by_uri)
}

/// `{ uri: [{ method, version }, …] }`, each URI's notifications in the order they arrived.
fn recorded_document_sync_log() -> Value {
    let mut by_uri: std::collections::BTreeMap<String, Vec<Value>> =
        std::collections::BTreeMap::new();
    for entry in recorded_document_sync() {
        by_uri
            .entry(entry.uri)
            .or_default()
            .push(json!({ "method": entry.method, "version": entry.version }));
    }
    json!(by_uri)
}

/// `{ uri: text }`, the latest contents the client announced for each URI.
fn recorded_document_texts() -> Value {
    let mut by_uri: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for entry in recorded_document_sync() {
        if let Some(text) = entry.text {
            by_uri.insert(entry.uri, text);
        }
    }
    json!(by_uri)
}

fn record_request_method(id: &Value, method: &str) {
    REQUEST_METHODS
        .lock()
        .unwrap()
        .get_or_insert_with(Vec::new)
        .push((id.clone(), method.to_string()));
}

/// Record a `$/cancelRequest` by the method of the request it names, which is what a test can
/// assert against — the ids the client chose are its own business.
fn record_cancellation(message: &Value) {
    let id = message
        .pointer("/params/id")
        .cloned()
        .unwrap_or(Value::Null);
    let method = REQUEST_METHODS
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
        .into_iter()
        .find(|(known, _)| *known == id)
        .map(|(_, method)| method)
        .unwrap_or_else(|| "unknown".to_string());
    CANCELLED_REQUESTS
        .lock()
        .unwrap()
        .get_or_insert_with(Vec::new)
        .push(method);
}

/// `[method, …]`, the requests the client asked to cancel, in arrival order.
fn recorded_cancellations() -> Value {
    json!(CANCELLED_REQUESTS
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default())
}

/// The progress a server reports while it still cannot answer.
///
/// `begin` once, then one `report` per unanswered hover with a rising percentage — the shape
/// `ServerChatter` folds, where the title arrives only with `begin` and later reports inherit it.
fn report_indexing_progress(served: u32) {
    const INDEXING_TOKEN: &str = "fake-lsp-indexing";
    if served == 0 {
        send(&json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": INDEXING_TOKEN,
                "value": { "kind": "begin", "title": "loading crate graph", "percentage": 0 },
            }
        }));
        return;
    }
    send(&json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": INDEXING_TOKEN,
            "value": { "kind": "report", "percentage": served.min(99) },
        }
    }));
}

/// How long the narrator waits before its first word.
///
/// A subscriber cannot attach until the handshake it is waiting on has returned, and it is handed
/// only what arrives after it attached — so a server that narrated its whole load in the same
/// breath as `initialized` would be narrating to nobody. This is orders of magnitude more than the
/// couple of awaits a host needs to get from `initialize` returning to subscribing, and it is why
/// the sequence below is worth asserting on at all.
const BEFORE_THE_FIRST_PHASE: std::time::Duration = std::time::Duration::from_millis(100);

/// How long each phase of the narrated load appears to take.
const BETWEEN_PHASES: std::time::Duration = std::time::Duration::from_millis(60);

/// The percentages the narrated load reports, in order.
const THE_PHASES_OF_A_NARRATED_LOAD: [u64; 3] = [25, 50, 75];

/// Narrate a crate-graph load, then report the graph queryable.
///
/// On a thread of its own because the load is something the server does *while* it keeps answering,
/// and a consumer that waits for quiescence must be able to read the phases on the way to it. The
/// `begin` deliberately carries no percentage: a title arrives only with `begin`, and a phase with
/// nothing to count is the shape a real server opens with.
fn narrate_a_crate_graph_load() {
    const LOADING_TOKEN: &str = "fake-lsp-crate-graph";
    std::thread::spawn(|| {
        std::thread::sleep(BEFORE_THE_FIRST_PHASE);
        send(&json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": LOADING_TOKEN,
                "value": { "kind": "begin", "title": "loading crate graph" },
            }
        }));
        for percentage in THE_PHASES_OF_A_NARRATED_LOAD {
            std::thread::sleep(BETWEEN_PHASES);
            send(&json!({
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": LOADING_TOKEN,
                    "value": { "kind": "report", "percentage": percentage },
                }
            }));
        }
        send(&json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": LOADING_TOKEN, "value": { "kind": "end" } }
        }));
        // The extension a real rust-analyzer signals a queryable graph with. Sent last, because
        // that is what it means: everything above was the load, and this is it being over.
        send(&json!({
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": true }
        }));
    });
}

fn flood_stderr() {
    let line = "fake_lsp: stderr flood\n".repeat(64);
    let stderr = std::io::stderr();
    let mut stderr = stderr.lock();
    let mut written = 0usize;
    while written < STDERR_FLOOD_BYTES {
        let _ = stderr.write_all(line.as_bytes());
        written += line.len();
    }
    let _ = stderr.flush();
}

/// The version a document-sync notification carries, if it declared one.
fn notified_version(message: &Value) -> Option<i64> {
    message
        .pointer("/params/textDocument/version")
        .and_then(Value::as_i64)
}

/// The contents a document-sync notification announced: `didOpen`'s whole text, or the full-text
/// content change a `didChange` carries. `didClose` announces none.
fn notified_text(message: &Value) -> Option<String> {
    message
        .pointer("/params/textDocument/text")
        .or_else(|| message.pointer("/params/contentChanges/0/text"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn notified_uri(message: &Value) -> String {
    message
        .pointer("/params/textDocument/uri")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn remember_initialize_params(params: Value) {
    *INITIALIZE_PARAMS.lock().unwrap() = Some(params);
}

fn remembered_initialize_params() -> Value {
    INITIALIZE_PARAMS
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(Value::Null)
}

fn handle_message(message: &Value, hang: bool, cold_hovers: u32, loads_crate_graph: bool) {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();
    if let Some(id) = &id {
        record_request_method(id, method);
    }

    match method {
        "initialize" => {
            remember_initialize_params(message.get("params").cloned().unwrap_or(Value::Null));
            reply(id, initialize_result())
        }
        // Replays what the client advertised at `initialize`.
        "tddy/initializeParams" => reply(id, remembered_initialize_params()),
        "initialized" => {
            if loads_crate_graph {
                narrate_a_crate_graph_load();
            }
        }
        "textDocument/didOpen" => {
            record_document_sync(message, method);
            send(&publish_diagnostics_notification(&notified_uri(message)));
        }
        "textDocument/didChange" | "textDocument/didClose" => record_document_sync(message, method),
        // Records which request the client asked to stop working on.
        "$/cancelRequest" => record_cancellation(message),
        // Replays the document versions the client sent, per URI, in arrival order.
        "tddy/documentVersions" => reply(id, recorded_document_versions()),
        // Replays the document-sync notifications themselves, method included.
        "tddy/documentSyncLog" => reply(id, recorded_document_sync_log()),
        // Replays the latest contents the client announced per URI.
        "tddy/documentTexts" => reply(id, recorded_document_texts()),
        // Replays the methods of the requests the client asked to cancel.
        "tddy/cancelledRequests" => reply(id, recorded_cancellations()),
        // Writes more to stderr than a pipe buffer holds, then replies.
        "tddy/floodStderr" => {
            flood_stderr();
            reply(id, Value::Null)
        }
        "textDocument/definition" => reply(id, definition_result()),
        "textDocument/references" => reply(id, references_result()),
        "textDocument/hover" => {
            let served = COLD_HOVERS_SERVED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if served < cold_hovers {
                report_indexing_progress(served);
                reply(id, Value::Null)
            } else {
                reply(id, hover_result())
            }
        }
        "textDocument/documentSymbol" => reply(id, document_symbol_result()),
        "textDocument/diagnostic" => reply(id, pull_diagnostic_result()),
        // rust-analyzer answers a request issued against a document it has since seen change
        // with `ContentModified` rather than a result. The client must surface it as an error.
        "textDocument/codeAction" => reply_error(id, FAKE_CONTENT_MODIFIED, "content modified"),
        // Sends nothing back, so the only thing that ends the wait is the request timeout.
        "tddy/neverAnswers" => {}
        "workspace/diagnostic" => reply(id, workspace_diagnostic_result()),
        "shutdown" => {
            if !hang {
                reply(id, Value::Null);
            }
        }
        "exit" => {
            if !hang {
                std::process::exit(0);
            }
        }
        _ => {
            // Unknown request: answer with null so the client never blocks.
            if id.is_some() {
                reply(id, Value::Null);
            }
        }
    }
}

/// The single source file location the fake reports for definition and the first
/// reference. Tests assert against these exact values.
const FAKE_LIB_URI: &str = "file:///workspace/src/lib.rs";
const FAKE_MAIN_URI: &str = "file:///workspace/src/main.rs";
const FAKE_HOVER_MARKDOWN: &str = "fn foo() -> u32";
const FAKE_SYMBOL_NAME: &str = "foo";
const FAKE_DIAGNOSTIC_MESSAGE: &str = "unused variable: `x`";
/// LSP `ContentModified`, the code rust-analyzer uses to say "ask again".
const FAKE_CONTENT_MODIFIED: i64 = -32801;

fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Value {
    json!({
        "start": {"line": sl, "character": sc},
        "end": {"line": el, "character": ec},
    })
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            // Honour what the client asked for in `general.positionEncodings`. A consumer that
            // counts bytes refuses a server that settled on utf-16 rather than resolving anchors
            // against the wrong unit, so a fake that stayed silent here could never be driven past
            // a handshake by such a consumer.
            "positionEncoding": "utf-8",
            "textDocumentSync": 1,
            "definitionProvider": true,
            "referencesProvider": true,
            "hoverProvider": true,
            "documentSymbolProvider": true,
            "diagnosticProvider": {
                "interFileDependencies": false,
                "workspaceDiagnostics": false
            }
        },
        "serverInfo": {"name": "fake_lsp", "version": "0.1.0"}
    })
}

fn definition_result() -> Value {
    json!([{ "uri": FAKE_LIB_URI, "range": range(10, 0, 10, 3) }])
}

fn references_result() -> Value {
    json!([
        { "uri": FAKE_LIB_URI, "range": range(10, 0, 10, 3) },
        { "uri": FAKE_MAIN_URI, "range": range(20, 4, 20, 7) },
    ])
}

fn hover_result() -> Value {
    json!({ "contents": { "kind": "markdown", "value": FAKE_HOVER_MARKDOWN } })
}

fn document_symbol_result() -> Value {
    json!([{
        "name": FAKE_SYMBOL_NAME,
        "kind": 12, // Function
        "range": range(10, 0, 12, 1),
        "selectionRange": range(10, 3, 10, 6),
    }])
}

fn one_diagnostic() -> Value {
    json!({
        "range": range(5, 4, 5, 9),
        "severity": 1, // Error
        "message": FAKE_DIAGNOSTIC_MESSAGE,
        "source": "rustc"
    })
}

fn pull_diagnostic_result() -> Value {
    json!({ "kind": "full", "items": [one_diagnostic()] })
}

fn workspace_diagnostic_result() -> Value {
    json!({
        "items": [{
            "kind": "full",
            "uri": FAKE_LIB_URI,
            "version": null,
            "items": [one_diagnostic()],
        }]
    })
}

fn publish_diagnostics_notification(uri: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": { "uri": uri, "diagnostics": [one_diagnostic()] }
    })
}

fn reply(id: Option<Value>, result: Value) {
    let Some(id) = id else { return };
    send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
}

/// Answer a request with a JSON-RPC `error` object in place of a `result`.
fn reply_error(id: Option<Value>, code: i64, message: &str) {
    let Some(id) = id else { return };
    send(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    }));
}

fn send(message: &Value) {
    let bytes = encode_message(message);
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let _ = stdout.write_all(&bytes);
    let _ = stdout.flush();
}
