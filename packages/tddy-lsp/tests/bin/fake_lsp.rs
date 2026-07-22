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

use std::io::{Read, Write};

use serde_json::{json, Value};
use tddy_lsp::protocol::{encode_message, FrameReader};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--exit-immediately") {
        std::process::exit(0);
    }
    let hang = args.iter().any(|a| a == "--hang");

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
            handle_message(&message, hang);
        }
    }
}

fn handle_message(message: &Value, hang: bool) {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();

    match method {
        "initialize" => reply(id, initialize_result()),
        "initialized" => {}
        "textDocument/didOpen" => {
            let uri = message
                .pointer("/params/textDocument/uri")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            send(&publish_diagnostics_notification(&uri));
        }
        "textDocument/definition" => reply(id, definition_result()),
        "textDocument/references" => reply(id, references_result()),
        "textDocument/hover" => reply(id, hover_result()),
        "textDocument/documentSymbol" => reply(id, document_symbol_result()),
        "textDocument/diagnostic" => reply(id, pull_diagnostic_result()),
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

fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Value {
    json!({
        "start": {"line": sl, "character": sc},
        "end": {"line": el, "character": ec},
    })
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
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

fn send(message: &Value) {
    let bytes = encode_message(message);
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let _ = stdout.write_all(&bytes);
    let _ = stdout.flush();
}
