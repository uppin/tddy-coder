//! A deterministic fake LSP server used by the `tddy-lsp` integration tests.
//!
//! It speaks `Content-Length`-framed JSON-RPC on stdin/stdout and answers a fixed set of
//! requests with known values (see the `FAKE_*` constants, which the tests assert
//! against). It is NOT part of the shipped product — only a test double so the tests
//! never need a real rust-analyzer.
//!
//! Modes (via argv):
//! - `--exit-immediately` — exit at startup (simulate a server that crashes on spawn).
//! - `--refuses-initialize` — answer `initialize` with error `-32603 refused`, so the handshake fails
//!   while the process is still running.
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
//! - `--answers-in-its-workspace` — report the definition and reference locations under the
//!   `rootUri` the client initialized with rather than under `file:///workspace`, so a consumer
//!   that maps locations back to paths relative to its root has a root to map them into. Same
//!   files, same ranges; off unless asked for, so every other mode is byte-identical.
//! - `--never-quiescent` — on `initialized`, report `experimental/serverStatus` with
//!   `quiescent: false`, open a `$/progress` phase titled `Building compile-time-deps` and report
//!   `build script num-bigint run` (no percentage) — and then say nothing, ever: no `end`, no
//!   `quiescent: true`. Hover answers non-null, as a real server's does while it runs build scripts.
//!   Wins over `--loads-crate-graph`. Lets a test wait on a server that never becomes ready.
//! - `--goes-busy-after-hovers N` — behave like `--loads-crate-graph`, and once N hovers have been
//!   answered, report `quiescent: false` and the same build-script progress, never ending: a tree
//!   that changed after a ready index. The report is sent *as the next hover arrives, before it is
//!   answered*, so that hover is already answered by a server that has said it is busy. Sent right
//!   after the Nth answer instead, whether a client that hovers again at once had heard it by the
//!   time it read that answer would be a race, and a consumer's test could not name which wait it
//!   was in. A crate-graph load still being narrated when the server goes busy says nothing more —
//!   above all no late `quiescent: true` — and the hover is answered at once all the same.
//! - `--narrates-while-loading` — once the server is busy on its build script, report a *new* build
//!   script (`build script crate-N run`) on the same phase every [`BETWEEN_BUILD_SCRIPT_REPORTS`],
//!   on a timer of its own rather than per request: a server that stays loading and keeps saying
//!   so, the way a long but healthy load does. Alone it behaves like `--never-quiescent`, so it is
//!   busy from the start; with `--goes-busy-after-hovers N` it is busy, and narrating, only once N
//!   hovers have been answered — past the warm-up, in the wait for type inference. Lets a test
//!   prove that a wait bounded on silence never ends a server that is still talking.
//! - `--hover-never-answers` — receive `textDocument/hover` and send no response, as
//!   `tddy/neverAnswers` does for its method. Every other request is answered.
//! - `--loads-crate-graph-on-signal <path>` — on `initialized`, report `experimental/serverStatus`
//!   with `quiescent: false` and begin two overlapping `$/progress` phases — `Fetching` at
//!   `metadata` 40% and `Indexing` at `120/310 (subject)` 38% — and then hold the load until a file
//!   exists at `<path>`, at which point both phases end and `quiescent: true` follows. The test
//!   decides when the graph is loaded, so "a watcher joined mid-load" and "a request was held until
//!   ready" need no sleeps. The load runs on a thread of its own, so every request is answered while
//!   it waits — as `--answers-in-its-workspace` says when combined with it. Wins over
//!   `--loads-crate-graph`.
//! - `--requests-configuration <section>` — on `initialized`, ask the client `workspace/configuration`
//!   for one item, `{ section: <section> }`, the way a server that reads its settings from the
//!   client does. `tddy/configurationAnswers` replays what the client answered, held until the
//!   answer has arrived — so a test that asks straight after the handshake does not race it.
//! - `--answers-rename` — answer `textDocument/rename` with a `WorkspaceEdit` that replaces the
//!   identifier under the requested position (letters, digits, `_` and `-`, in the text the client
//!   last announced for that document) with `newName`. Without it a rename is answered `null`, as
//!   every unknown request is.
//! - `--code-actions <path>` — answer `textDocument/codeAction` from the JSON file at `<path>`,
//!   `{ "actions": [CodeAction…], "resolved": { <title>: WorkspaceEdit }, "command_edits":
//!   { <command>: [WorkspaceEdit…] } }` (every key optional), instead of the `ContentModified`
//!   error: every action, whatever range or `context.only` was asked. With `resolved` non-empty the
//!   server advertises `codeActionProvider.resolveProvider` and answers `codeAction/resolve` with the
//!   action given that title's `edit`. With `command_edits` it advertises `executeCommandProvider`,
//!   and a `workspace/executeCommand` naming a command there sends one `workspace/applyEdit`
//!   request per listed edit, one at a time, each sent only once the client answered the one
//!   before, and answers the command (`null`) only after the client answered the last; a command
//!   with no entry is answered `null` at once, having sent no edit. `tddy/applyEditAnswers`
//!   replays what the client answered those requests, in order; `tddy/codeActionRequests` the
//!   params of every `textDocument/codeAction` received.
//! - `--diagnoses <marker>` — the diagnostics of a document are computed from the text the client
//!   last announced for it: one error (`severity` 1, source `fake`, message `unexpected <marker>`)
//!   per occurrence of `<marker>`, at its range. Published on every `didOpen` and `didChange`, and
//!   the answer to a pull `textDocument/diagnostic`, instead of the one fixed diagnostic.
//!
//! Every mode above is off unless asked for, so adding one leaves a server started without it
//! byte-identical to what it was.
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
//! `tddy/documentLanguages` replays the `languageId` each URI was last opened as, as
//! `{ uri: languageId }`. A test asserts what a document was announced as, which is the only place a
//! caller's choice of `languageId` is observable.
//!
//! `tddy/cancelledRequests` replays the methods of the requests the client sent `$/cancelRequest`
//! for, in arrival order. A test asserts that abandoning a request told the server to stop working
//! on it, rather than only that the caller stopped waiting.
//!
//! `tddy/watchedFileChanges` replays the `workspace/didChangeWatchedFiles` changes the client sent,
//! flattened in arrival order, as `[{ uri, type }, …]`. A test asserts that a host told the server
//! about files that changed on disk behind it.
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
    REFUSES_INITIALIZE.store(
        args.iter().any(|a| a == "--refuses-initialize"),
        std::sync::atomic::Ordering::SeqCst,
    );
    let narrates_while_loading = args.iter().any(|a| a == "--narrates-while-loading");
    NARRATES_WHILE_LOADING.store(narrates_while_loading, std::sync::atomic::Ordering::SeqCst);
    let goes_busy_after_hovers = args.iter().any(|a| a == "--goes-busy-after-hovers");
    // Narration alone is a server busy from the start; with `--goes-busy-after-hovers` it waits to
    // go busy like that mode does, and narrates from then on.
    let never_quiescent = (narrates_while_loading && !goes_busy_after_hovers)
        || args.iter().any(|a| a == "--never-quiescent");
    // `--never-quiescent` wins: a server that never finishes loading must not narrate a finished one.
    let loads_crate_graph = !never_quiescent
        && (args.iter().any(|a| a == "--loads-crate-graph") || goes_busy_after_hovers);
    *LOADS_CRATE_GRAPH_ON_SIGNAL.lock().unwrap() = args
        .iter()
        .position(|a| a == "--loads-crate-graph-on-signal")
        .and_then(|at| args.get(at + 1))
        .map(std::path::PathBuf::from);
    NEVER_QUIESCENT.store(never_quiescent, std::sync::atomic::Ordering::SeqCst);
    HOVER_NEVER_ANSWERS.store(
        args.iter().any(|a| a == "--hover-never-answers"),
        std::sync::atomic::Ordering::SeqCst,
    );
    if let Some(after) = args
        .iter()
        .position(|a| a == "--goes-busy-after-hovers")
        .and_then(|at| args.get(at + 1))
        .and_then(|value| value.parse::<u32>().ok())
    {
        GOES_BUSY_AFTER_HOVERS.store(after, std::sync::atomic::Ordering::SeqCst);
    }
    ANSWERS_IN_ITS_WORKSPACE.store(
        args.iter().any(|a| a == "--answers-in-its-workspace"),
        std::sync::atomic::Ordering::SeqCst,
    );
    *REQUESTS_CONFIGURATION.lock().unwrap() = args
        .iter()
        .position(|a| a == "--requests-configuration")
        .and_then(|at| args.get(at + 1))
        .cloned();
    ANSWERS_RENAME.store(
        args.iter().any(|a| a == "--answers-rename"),
        std::sync::atomic::Ordering::SeqCst,
    );
    if let Some(path) = args
        .iter()
        .position(|a| a == "--code-actions")
        .and_then(|at| args.get(at + 1))
    {
        let text = std::fs::read_to_string(path).expect("--code-actions names a readable file");
        *CODE_ACTIONS.lock().unwrap() =
            Some(serde_json::from_str(&text).expect("--code-actions names a JSON file"));
    }
    *DIAGNOSES.lock().unwrap() = args
        .iter()
        .position(|a| a == "--diagnoses")
        .and_then(|at| args.get(at + 1))
        .cloned();
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

/// Set by `--requests-configuration <section>`: the section the server asks the client for.
static REQUESTS_CONFIGURATION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The id of the `workspace/configuration` request this server sends the client.
const CONFIGURATION_REQUEST_ID: &str = "fake-lsp-configuration";

/// What the client answered to the `workspace/configuration` request, once it has.
static CONFIGURATION_ANSWERS: std::sync::Mutex<Option<Vec<Value>>> = std::sync::Mutex::new(None);

/// A `tddy/configurationAnswers` that arrived before the client's answer did, to reply to once it
/// arrives.
static PENDING_CONFIGURATION_REPLAY: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

/// Set by `--code-actions <path>`: the file's `{ actions, resolved, command_edits }`.
static CODE_ACTIONS: std::sync::Mutex<Option<Value>> = std::sync::Mutex::new(None);

/// Set by `--diagnoses <marker>`: the text whose every occurrence is an error.
static DIAGNOSES: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The prefix of the ids of the `workspace/applyEdit` requests this server sends the client.
const APPLY_EDIT_REQUEST_ID: &str = "fake-lsp-apply-edit-";

/// A `workspace/executeCommand` waiting on the client's answers: its id and the edits still to send.
static RUNNING_COMMAND: std::sync::Mutex<Option<(Value, Vec<Value>)>> = std::sync::Mutex::new(None);

/// Every answer the client gave this server's `workspace/applyEdit` requests, in order.
static APPLY_EDIT_ANSWERS: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());

/// The params of every `textDocument/codeAction` received, in order.
static CODE_ACTION_REQUESTS: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());

/// Set by `--answers-rename`: `textDocument/rename` is answered with an edit.
static ANSWERS_RENAME: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set by `--answers-in-its-workspace`: locations are reported under the client's `rootUri`.
static ANSWERS_IN_ITS_WORKSPACE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Set by `--loads-crate-graph-on-signal <path>`: the file whose appearance ends the crate-graph load.
static LOADS_CRATE_GRAPH_ON_SIGNAL: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

/// Set by `--narrates-while-loading`: a busy server reports a new build script on a timer.
static NARRATES_WHILE_LOADING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Set by `--refuses-initialize`: `initialize` is answered with an error.
static REFUSES_INITIALIZE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Set by `--never-quiescent`: the server reports itself busy and never reports itself ready.
static NEVER_QUIESCENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set by `--hover-never-answers`: `textDocument/hover` receives no response.
static HOVER_NEVER_ANSWERS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Set by `--goes-busy-after-hovers N`: the hover after which the server goes busy again. Zero
/// means the mode is off (a server that went busy "after no hovers" is `--never-quiescent`).
static GOES_BUSY_AFTER_HOVERS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

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

/// Every `workspace/didChangeWatchedFiles` change received, flattened in arrival order.
static WATCHED_FILE_CHANGES: std::sync::Mutex<Vec<Value>> = std::sync::Mutex::new(Vec::new());

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
    language_id: Option<String>,
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
            language_id: message
                .pointer("/params/textDocument/languageId")
                .and_then(Value::as_str)
                .map(str::to_string),
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

/// `{ uri: languageId }`, the `languageId` each URI was last opened as.
fn recorded_document_languages() -> Value {
    let mut by_uri: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for entry in recorded_document_sync() {
        if let Some(language_id) = entry.language_id {
            by_uri.insert(entry.uri, language_id);
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
        if !send_unless_gone_busy(&json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": LOADING_TOKEN,
                "value": { "kind": "begin", "title": "loading crate graph" },
            }
        })) {
            return;
        }
        for percentage in THE_PHASES_OF_A_NARRATED_LOAD {
            std::thread::sleep(BETWEEN_PHASES);
            if !send_unless_gone_busy(&json!({
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": LOADING_TOKEN,
                    "value": { "kind": "report", "percentage": percentage },
                }
            })) {
                return;
            }
        }
        if !send_unless_gone_busy(&json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": LOADING_TOKEN, "value": { "kind": "end" } }
        })) {
            return;
        }
        // The extension a real rust-analyzer signals a queryable graph with. Sent last, because
        // that is what it means: everything above was the load, and this is it being over.
        send_unless_gone_busy(&json!({
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": true }
        }));
    });
}

/// How often a load held on a signal file (`--loads-crate-graph-on-signal`) looks for it.
const BETWEEN_SIGNAL_CHECKS: std::time::Duration = std::time::Duration::from_millis(20);

/// Begin a crate-graph load with two overlapping phases, hold it until `signal` exists, then end
/// both phases and report the graph queryable.
///
/// Waits [`BEFORE_THE_FIRST_PHASE`] before its first word, for the same reason the narrated load
/// does: a host attaches its watcher only once the handshake has returned. Each phase opens with a
/// title and no message, then reports a message and a percentage — the shape a real rust-analyzer's
/// `Fetching` and `Indexing` take, overlapping as theirs do.
fn load_the_crate_graph_once_signalled(signal: std::path::PathBuf) {
    const FETCHING_TOKEN: &str = "fake-lsp-fetching";
    const INDEXING_TOKEN: &str = "fake-lsp-indexing-on-signal";
    std::thread::spawn(move || {
        std::thread::sleep(BEFORE_THE_FIRST_PHASE);
        send(&json!({
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": false }
        }));
        send(&progress(
            FETCHING_TOKEN,
            json!({ "kind": "begin", "title": "Fetching", "percentage": 0 }),
        ));
        send(&progress(
            INDEXING_TOKEN,
            json!({ "kind": "begin", "title": "Indexing", "percentage": 0 }),
        ));
        send(&progress(
            FETCHING_TOKEN,
            json!({ "kind": "report", "message": "metadata", "percentage": 40 }),
        ));
        send(&progress(
            INDEXING_TOKEN,
            json!({ "kind": "report", "message": "120/310 (subject)", "percentage": 38 }),
        ));
        while !signal.exists() {
            std::thread::sleep(BETWEEN_SIGNAL_CHECKS);
        }
        send(&progress(FETCHING_TOKEN, json!({ "kind": "end" })));
        send(&progress(INDEXING_TOKEN, json!({ "kind": "end" })));
        send(&json!({
            "jsonrpc": "2.0",
            "method": "experimental/serverStatus",
            "params": { "health": "ok", "quiescent": true }
        }));
    });
}

/// A `$/progress` notification carrying `value` for `token`.
fn progress(token: &str, value: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": { "token": token, "value": value }
    })
}

/// Whether the server has gone busy ([`go_busy_on_a_build_script`]), behind the lock both it and
/// the narrated load send under — so no line of the load can slip out after the busy report.
static GONE_BUSY: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// Send one line of the narrated crate-graph load, unless the server has gone busy since; `false`
/// once it has, for the load to stop.
///
/// `--goes-busy-after-hovers` is a tree that changed after a ready index. A client can be through
/// its warm-up on hover alone before the load has finished narrating, so the busy report can come
/// first — and the load's own `quiescent: true` sent after it would read as a server that went busy
/// and then, for no reason, ready. A busy server supersedes the load instead: the load says nothing
/// more. Nothing waits, so no request's answer is held while the load plays out.
fn send_unless_gone_busy(message: &Value) -> bool {
    let gone_busy = GONE_BUSY.lock().unwrap();
    if *gone_busy {
        return false;
    }
    send(message);
    true
}

/// Report the server busy on a build script, and never say it is done.
///
/// `quiescent: false` first, then a phase with a title and one report that carries no percentage —
/// the shape of the `working: build script num-bigint run` a real rust-analyzer stays on while a
/// build script runs. Nothing ends the phase and nothing reports quiescence afterwards.
fn go_busy_on_a_build_script() {
    const BUSY_TOKEN: &str = "fake-lsp-build-script";
    let mut gone_busy = GONE_BUSY.lock().unwrap();
    *gone_busy = true;
    send(&json!({
        "jsonrpc": "2.0",
        "method": "experimental/serverStatus",
        "params": { "health": "ok", "quiescent": false }
    }));
    send(&json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": BUSY_TOKEN,
            "value": { "kind": "begin", "title": "Building compile-time-deps" },
        }
    }));
    send(&json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": BUSY_TOKEN,
            "value": { "kind": "report", "message": "build script num-bigint run" },
        }
    }));
    if NARRATES_WHILE_LOADING.load(std::sync::atomic::Ordering::SeqCst) {
        keep_narrating_build_scripts();
    }
}

/// How often a server narrating its load (`--narrates-while-loading`) reports a new build script.
const BETWEEN_BUILD_SCRIPT_REPORTS: std::time::Duration = std::time::Duration::from_millis(250);

/// Set once the narrator thread has started, so going busy twice does not start a second one.
static NARRATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Report a new build script every [`BETWEEN_BUILD_SCRIPT_REPORTS`], for ever.
///
/// On a thread of its own, so the server talks between requests as a real loading one does, and
/// a client measuring its silence sees new words however often or seldom it asks.
fn keep_narrating_build_scripts() {
    if NARRATING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        for reported in 0.. {
            std::thread::sleep(BETWEEN_BUILD_SCRIPT_REPORTS);
            report_another_build_script(reported);
        }
    });
}

/// Report the `reported`-th build script on the busy phase — new words, so a client measuring how
/// long the server has been silent sees it talking.
fn report_another_build_script(reported: u32) {
    send(&json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": "fake-lsp-build-script",
            "value": { "kind": "report", "message": format!("build script crate-{reported} run") },
        }
    }));
}

/// Whether `hovers_answered` hovers have been answered and `--goes-busy-after-hovers` names that
/// many, so the hover now arriving is the first the server meets busy.
fn the_server_goes_busy_before_this_hover(hovers_answered: u32) -> bool {
    let after = GOES_BUSY_AFTER_HOVERS.load(std::sync::atomic::Ordering::SeqCst);
    after != 0 && hovers_answered == after
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

/// Ask the client for the `--requests-configuration` section, if one was given.
fn request_configuration() {
    let Some(section) = REQUESTS_CONFIGURATION.lock().unwrap().clone() else {
        return;
    };
    send(&json!({
        "jsonrpc": "2.0",
        "id": CONFIGURATION_REQUEST_ID,
        "method": "workspace/configuration",
        "params": { "items": [{ "section": section }] }
    }));
}

/// A response from the client: the answer to this server's `workspace/configuration` request is
/// kept, and replayed to a `tddy/configurationAnswers` that was waiting for it.
fn record_client_response(message: &Value) {
    if message
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| id.starts_with(APPLY_EDIT_REQUEST_ID))
    {
        APPLY_EDIT_ANSWERS
            .lock()
            .unwrap()
            .push(message.get("result").cloned().unwrap_or(Value::Null));
        send_the_next_edit_or_answer_the_command();
        return;
    }
    if message.get("id").and_then(Value::as_str) != Some(CONFIGURATION_REQUEST_ID) {
        return;
    }
    let answer = message.get("result").cloned().unwrap_or(Value::Null);
    CONFIGURATION_ANSWERS
        .lock()
        .unwrap()
        .get_or_insert_with(Vec::new)
        .push(answer);
    if let Some(waiting) = PENDING_CONFIGURATION_REPLAY.lock().unwrap().take() {
        replay_configuration_answers(Some(waiting));
    }
}

/// Reply with every configuration answer the client gave, or hold the reply until it has given one.
fn replay_configuration_answers(id: Option<Value>) {
    let answers = CONFIGURATION_ANSWERS.lock().unwrap().clone();
    match answers {
        Some(answers) => reply(id, json!(answers)),
        None => *PENDING_CONFIGURATION_REPLAY.lock().unwrap() = id,
    }
}

/// The `--code-actions` file's value at `key`, or null.
fn code_actions_entry(key: &str) -> Value {
    CODE_ACTIONS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|file| file.get(key).cloned())
        .unwrap_or(Value::Null)
}

/// `textDocument/codeAction` under `--code-actions`: every action the file lists.
fn code_action_result(message: &Value) -> Value {
    CODE_ACTION_REQUESTS
        .lock()
        .unwrap()
        .push(message.get("params").cloned().unwrap_or(Value::Null));
    match code_actions_entry("actions") {
        Value::Null => json!([]),
        actions => actions,
    }
}

/// `codeAction/resolve` under `--code-actions`: the action, given the `resolved` edit of its title.
fn resolved_code_action(message: &Value) -> Value {
    let mut action = message.get("params").cloned().unwrap_or(Value::Null);
    let title = action
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(edit) = title.and_then(|title| code_actions_entry("resolved").get(&title).cloned())
    {
        action["edit"] = edit;
    }
    action
}

/// `workspace/executeCommand` under `--code-actions`: start sending the command's edits, or answer
/// at once when it has none.
fn run_command(id: Option<Value>, message: &Value) {
    let command = message
        .pointer("/params/command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let edits = code_actions_entry("command_edits")
        .get(&command)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(id) = id else { return };
    *RUNNING_COMMAND.lock().unwrap() = Some((id, edits.into_iter().rev().collect()));
    send_the_next_edit_or_answer_the_command();
}

/// Send the running command's next `workspace/applyEdit`, or answer the command when none is left.
fn send_the_next_edit_or_answer_the_command() {
    let mut running = RUNNING_COMMAND.lock().unwrap();
    let Some((id, remaining)) = running.as_mut() else {
        return;
    };
    match remaining.pop() {
        Some(edit) => {
            let sent = APPLY_EDIT_ANSWERS.lock().unwrap().len();
            send(&json!({
                "jsonrpc": "2.0",
                "id": format!("{APPLY_EDIT_REQUEST_ID}{sent}"),
                "method": "workspace/applyEdit",
                "params": { "label": "fake command", "edit": edit }
            }));
        }
        None => {
            let id = id.clone();
            *running = None;
            drop(running);
            reply(Some(id), Value::Null);
        }
    }
}

/// The diagnostics `--diagnoses <marker>` computes for `uri`: one error per occurrence of the
/// marker in the text last announced for it.
fn diagnosed(uri: &str, marker: &str) -> Vec<Value> {
    let text = recorded_document_texts()
        .get(uri)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    let mut found = Vec::new();
    for (line, source) in text.split('\n').enumerate() {
        for (at, _) in source.match_indices(marker) {
            found.push(json!({
                "range": range(line as u32, at as u32, line as u32, (at + marker.len()) as u32),
                "severity": 1,
                "message": format!("unexpected {marker}"),
                "source": "fake"
            }));
        }
    }
    found
}

/// The diagnostics a document has: computed under `--diagnoses`, the one fixed one otherwise.
fn diagnostics_of(uri: &str) -> Vec<Value> {
    match DIAGNOSES.lock().unwrap().clone() {
        Some(marker) => diagnosed(uri, &marker),
        None => vec![one_diagnostic()],
    }
}

/// The edit `--answers-rename` gives a `textDocument/rename`: the identifier under the position, in
/// the text last announced for the document, replaced by `newName`; `null` when the document was
/// never announced or no identifier is there.
fn rename_result(message: &Value) -> Value {
    let uri = notified_uri(message);
    let line = message
        .pointer("/params/position/line")
        .and_then(Value::as_u64);
    let character = message
        .pointer("/params/position/character")
        .and_then(Value::as_u64);
    let new_name = message
        .pointer("/params/newName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let text = recorded_document_texts()
        .get(&uri)
        .and_then(Value::as_str)
        .map(str::to_string);
    let (Some(text), Some(line), Some(character)) = (text, line, character) else {
        return Value::Null;
    };
    let Some(source_line) = text.split('\n').nth(line as usize) else {
        return Value::Null;
    };
    let Some((start, end)) = identifier_around(source_line, character as usize) else {
        return Value::Null;
    };
    json!({
        "changes": {
            uri: [{ "range": range(line as u32, start as u32, line as u32, end as u32), "newText": new_name }]
        }
    })
}

/// The byte range of the identifier that contains byte `at` of `line`.
fn identifier_around(line: &str, at: usize) -> Option<(usize, usize)> {
    let is_part = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-';
    let bytes = line.as_bytes();
    if at >= bytes.len() || !is_part(bytes[at]) {
        return None;
    }
    let start = (0..at)
        .rev()
        .take_while(|&i| is_part(bytes[i]))
        .last()
        .unwrap_or(at);
    let end = (at..bytes.len())
        .find(|&i| !is_part(bytes[i]))
        .unwrap_or(bytes.len());
    Some((start, end))
}

fn handle_message(message: &Value, hang: bool, cold_hovers: u32, loads_crate_graph: bool) {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();
    // A response from the client to a request this server sent: it carries no method, and must
    // not be answered as an unknown request.
    if method.is_empty() && (message.get("result").is_some() || message.get("error").is_some()) {
        record_client_response(message);
        return;
    }
    if let Some(id) = &id {
        record_request_method(id, method);
    }

    match method {
        "initialize" if REFUSES_INITIALIZE.load(std::sync::atomic::Ordering::SeqCst) => {
            reply_error(id, -32603, "refused")
        }
        "initialize" => {
            remember_initialize_params(message.get("params").cloned().unwrap_or(Value::Null));
            reply(id, initialize_result())
        }
        // Replays what the client advertised at `initialize`.
        "tddy/initializeParams" => reply(id, remembered_initialize_params()),
        "initialized" => {
            if NEVER_QUIESCENT.load(std::sync::atomic::Ordering::SeqCst) {
                // Straight away, with the handshake: a client that asks its first question has
                // to find a server that has already said it is busy, or it would take the silence
                // before the first status for a ready one.
                go_busy_on_a_build_script();
            } else if let Some(signal) = LOADS_CRATE_GRAPH_ON_SIGNAL.lock().unwrap().clone() {
                load_the_crate_graph_once_signalled(signal);
            } else if loads_crate_graph {
                narrate_a_crate_graph_load();
            }
            request_configuration();
        }
        "textDocument/didOpen" => {
            record_document_sync(message, method);
            send(&publish_diagnostics_notification(&notified_uri(message)));
        }
        "textDocument/didChange" if DIAGNOSES.lock().unwrap().is_some() => {
            record_document_sync(message, method);
            send(&publish_diagnostics_notification(&notified_uri(message)));
        }
        "textDocument/didChange" | "textDocument/didClose" => record_document_sync(message, method),
        // Records which request the client asked to stop working on.
        "$/cancelRequest" => record_cancellation(message),
        "workspace/didChangeWatchedFiles" => WATCHED_FILE_CHANGES.lock().unwrap().extend(
            message
                .pointer("/params/changes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        ),
        // Replays the watched-file changes the client announced, in arrival order.
        "tddy/watchedFileChanges" => reply(id, json!(WATCHED_FILE_CHANGES.lock().unwrap().clone())),
        // Replays the document versions the client sent, per URI, in arrival order.
        "tddy/documentVersions" => reply(id, recorded_document_versions()),
        // Replays the document-sync notifications themselves, method included.
        "tddy/documentSyncLog" => reply(id, recorded_document_sync_log()),
        // Replays the latest contents the client announced per URI.
        "tddy/documentTexts" => reply(id, recorded_document_texts()),
        // Replays the `languageId` each URI was last opened as.
        "tddy/documentLanguages" => reply(id, recorded_document_languages()),
        // Replays what the client answered to this server's `workspace/configuration` request.
        "tddy/configurationAnswers" => replay_configuration_answers(id),
        "textDocument/rename" if ANSWERS_RENAME.load(std::sync::atomic::Ordering::SeqCst) => {
            reply(id, rename_result(message))
        }
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
            if the_server_goes_busy_before_this_hover(served) {
                go_busy_on_a_build_script();
            }
            if HOVER_NEVER_ANSWERS.load(std::sync::atomic::Ordering::SeqCst) {
                // Sends nothing back: only the caller's own decision ends the wait.
            } else if served < cold_hovers {
                report_indexing_progress(served);
                reply(id, Value::Null)
            } else {
                reply(id, hover_result())
            }
        }
        "textDocument/documentSymbol" => reply(id, document_symbol_result()),
        "textDocument/diagnostic" => reply(id, pull_diagnostic_result(&notified_uri(message))),
        // rust-analyzer answers a request issued against a document it has since seen change
        // with `ContentModified` rather than a result. The client must surface it as an error.
        "textDocument/codeAction" if CODE_ACTIONS.lock().unwrap().is_some() => {
            reply(id, code_action_result(message))
        }
        "codeAction/resolve" if CODE_ACTIONS.lock().unwrap().is_some() => {
            reply(id, resolved_code_action(message))
        }
        "workspace/executeCommand" if CODE_ACTIONS.lock().unwrap().is_some() => {
            run_command(id, message)
        }
        // Replays what the client answered this server's `workspace/applyEdit` requests.
        "tddy/applyEditAnswers" => reply(id, json!(APPLY_EDIT_ANSWERS.lock().unwrap().clone())),
        // Replays the params of every `textDocument/codeAction` received.
        "tddy/codeActionRequests" => reply(id, json!(CODE_ACTION_REQUESTS.lock().unwrap().clone())),
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
/// The root [`FAKE_LIB_URI`] and [`FAKE_MAIN_URI`] sit under unless `--answers-in-its-workspace`
/// re-roots them.
const FAKE_ROOT_URI: &str = "file:///workspace";
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
    let mut result = fixed_initialize_result();
    if CODE_ACTIONS.lock().unwrap().is_some() {
        let resolves = code_actions_entry("resolved")
            .as_object()
            .is_some_and(|resolved| !resolved.is_empty());
        result["capabilities"]["codeActionProvider"] = json!({ "resolveProvider": resolves });
        let commands: Vec<String> = code_actions_entry("command_edits")
            .as_object()
            .map(|edits| edits.keys().cloned().collect())
            .unwrap_or_default();
        if !commands.is_empty() {
            result["capabilities"]["executeCommandProvider"] = json!({ "commands": commands });
        }
    }
    result
}

fn fixed_initialize_result() -> Value {
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
    json!([{ "uri": reported(FAKE_LIB_URI), "range": range(10, 0, 10, 3) }])
}

fn references_result() -> Value {
    json!([
        { "uri": reported(FAKE_LIB_URI), "range": range(10, 0, 10, 3) },
        { "uri": reported(FAKE_MAIN_URI), "range": range(20, 4, 20, 7) },
    ])
}

/// A fixed location's URI as this run reports it: unchanged, or re-rooted under the `rootUri` the
/// client initialized with when `--answers-in-its-workspace` was given.
fn reported(uri: &str) -> String {
    if !ANSWERS_IN_ITS_WORKSPACE.load(std::sync::atomic::Ordering::SeqCst) {
        return uri.to_string();
    }
    let root = remembered_initialize_params()
        .get("rootUri")
        .and_then(Value::as_str)
        .unwrap_or(FAKE_ROOT_URI)
        .trim_end_matches('/')
        .to_string();
    uri.replacen(FAKE_ROOT_URI, &root, 1)
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

fn pull_diagnostic_result(uri: &str) -> Value {
    json!({ "kind": "full", "items": diagnostics_of(uri) })
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
        "params": { "uri": uri, "diagnostics": diagnostics_of(uri) }
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
