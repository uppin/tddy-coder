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
            handle_message(&message, hang, cold_hovers);
        }
    }
}

/// The `initialize` params as received, so `tddy/initializeParams` can replay them.
static INITIALIZE_PARAMS: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

/// Every document version received, per URI, in arrival order.
static DOCUMENT_VERSIONS: std::sync::Mutex<Option<Vec<(String, i64)>>> =
    std::sync::Mutex::new(None);

/// How many `textDocument/hover` requests have been answered with `null` so far.
static COLD_HOVERS_SERVED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Enough stderr to exceed a pipe buffer on every platform this runs on (64 KiB is the usual
/// ceiling), so an undrained pipe blocks rather than merely filling.
const STDERR_FLOOD_BYTES: usize = 512 * 1024;

fn record_document_version(uri: &str, version: i64) {
    DOCUMENT_VERSIONS
        .lock()
        .unwrap()
        .get_or_insert_with(Vec::new)
        .push((uri.to_string(), version));
}

/// `{ uri: [version, …] }`, each URI's versions in the order they arrived.
fn recorded_document_versions() -> Value {
    let recorded = DOCUMENT_VERSIONS
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    let mut by_uri: std::collections::BTreeMap<String, Vec<i64>> =
        std::collections::BTreeMap::new();
    for (uri, version) in recorded {
        by_uri.entry(uri).or_default().push(version);
    }
    json!(by_uri)
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

fn handle_message(message: &Value, hang: bool, cold_hovers: u32) {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();

    match method {
        "initialize" => {
            remember_initialize_params(message.get("params").cloned().unwrap_or(Value::Null));
            reply(id, initialize_result())
        }
        // Replays what the client advertised at `initialize`.
        "tddy/initializeParams" => reply(id, remembered_initialize_params()),
        "initialized" => {}
        "textDocument/didOpen" => {
            let uri = notified_uri(message);
            if let Some(version) = notified_version(message) {
                record_document_version(&uri, version);
            }
            send(&publish_diagnostics_notification(&uri));
        }
        "textDocument/didChange" | "textDocument/didClose" => {
            let uri = notified_uri(message);
            if let Some(version) = notified_version(message) {
                record_document_version(&uri, version);
            }
        }
        // Replays the document versions the client sent, per URI, in arrival order.
        "tddy/documentVersions" => reply(id, recorded_document_versions()),
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
