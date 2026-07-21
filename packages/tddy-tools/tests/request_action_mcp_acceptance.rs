//! Acceptance tests: no-bash mode session-action tools (`request_action`, `list_actions`,
//! `invoke_action`) exposed over the real `tddy-tools --mcp` stdio wire.
//!
//! Feature: docs/ft/coder/no-bash-mode.md
//!
//! Mirrors `subagent_mcp_acceptance.rs`: spawn the actual compiled `tddy-tools` binary and speak
//! the real newline-delimited JSON-RPC wire. The action-author model is a wiremock
//! `/v1/chat/completions` endpoint; the host side of `EstablishAction` is a fake
//! `connection.ConnectionService/ExecuteTool` service hosted on a Unix socket in this test
//! process (the same protocol `dispatch_via_sandbox_ipc` speaks to the sandbox runner relay).

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use prost::Message;
use serde_json::{json, Value};
use tddy_rpc::{RpcMessage, RpcResult, RpcService};
use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

const VALID_MANIFEST: &str = "\
version: 1
id: run-core-tests
summary: Run the tddy-core test suite
architecture: native
command: [cargo, test, -p, tddy-core]
";

// ─── MCP wire harness (same shape as subagent_mcp_acceptance.rs) ───────────────

fn spawn_mcp_server(env: &[(&str, &str)]) -> Child {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"));
    cmd.arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.spawn().expect("spawn tddy-tools --mcp")
}

async fn send_json_line(stdin: &mut ChildStdin, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    tokio::time::timeout(IO_TIMEOUT, stdin.write_all(line.as_bytes()))
        .await
        .expect("write to tddy-tools stdin timed out")
        .expect("write to tddy-tools stdin");
}

async fn read_json_line(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(IO_TIMEOUT, stdout.read_line(&mut line))
        .await
        .expect("read from tddy-tools stdout timed out")
        .expect("read from tddy-tools stdout");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"))
}

async fn initialize_mcp_session(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) {
    send_json_line(
        stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tddy-red-test-client", "version": "0.0.1"}
            }
        }),
    )
    .await;
    read_json_line(stdout).await;
    send_json_line(
        stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
}

async fn call_tool(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: i64,
    name: &str,
    arguments: Value,
) -> Value {
    send_json_line(
        stdin,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }),
    )
    .await;
    read_json_line(stdout).await
}

fn tool_result_json(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| {
            panic!("tools/call result must carry a text content block; got: {response}")
        });
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tool result text {text:?} was not valid JSON: {e}"))
}

async fn tools_list_names(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
) -> Vec<String> {
    send_json_line(
        stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
    )
    .await;
    let response = read_json_line(stdout).await;
    response["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list must return a tools array, got: {response}"))
        .iter()
        .map(|t| t["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

// ─── Fake host-side ExecuteTool service on a Unix socket ───────────────────────

type RecordedDispatches = Arc<Mutex<Vec<(String, String)>>>;

/// Records every `(tool_name, args_json)` and answers with a canned per-tool result — the
/// host-relay stand-in on the other side of `TDDY_SANDBOX_TOOL_IPC`.
struct FakeHostToolService {
    recorded: RecordedDispatches,
}

#[async_trait]
impl RpcService for FakeHostToolService {
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult {
        assert_eq!(service, "connection.ConnectionService");
        assert_eq!(method, "ExecuteTool");
        let request = ExecuteToolRequest::decode(message.payload.as_ref())
            .expect("decode ExecuteToolRequest");
        self.recorded
            .lock()
            .unwrap()
            .push((request.tool_name.clone(), request.args_json.clone()));
        let result_json = match request.tool_name.as_str() {
            "EstablishAction" => json!({
                "id": "run-core-tests",
                "summary": "Run the tddy-core test suite",
                "path": "/host/session/actions/run-core-tests.yaml",
                "has_input_schema": false,
            })
            .to_string(),
            other => json!({"echo": other}).to_string(),
        };
        let response = ExecuteToolResponse {
            result_json,
            is_error: false,
            error_message: String::new(),
            job_id: String::new(),
            job_running: false,
        };
        RpcResult::Unary(Ok(response.encode_to_vec()))
    }
}

/// Host a fake ExecuteTool service on a fresh Unix socket; each inbound connection (one per
/// `dispatch_via_sandbox_ipc` call) gets its own stdio-RPC endpoint over the stream.
fn start_fake_host_relay(dir: &std::path::Path) -> (std::path::PathBuf, RecordedDispatches) {
    let socket_path = dir.join("tool-ipc.sock");
    let recorded: RecordedDispatches = Arc::new(Mutex::new(Vec::new()));
    let listener = std::os::unix::net::UnixListener::bind(&socket_path).expect("bind tool ipc");
    listener.set_nonblocking(true).expect("nonblocking");
    let listener = tokio::net::UnixListener::from_std(listener).expect("tokio listener");
    let recorded_for_task = recorded.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let (read_half, write_half) = tokio::io::split(stream);
            let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                read_half,
                write_half,
                FakeHostToolService {
                    recorded: recorded_for_task.clone(),
                },
            );
            tokio::spawn(endpoint.run());
        }
    });
    (socket_path, recorded)
}

fn author_def_json(base_url: &str) -> String {
    json!([{
        "name": "action-author",
        "model": "gemma4:e4b-mlx",
        "base_url": base_url,
        "replaces": ["Shell"],
    }])
    .to_string()
}

fn final_answer_response(answer: &str) -> Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }]
    })
}

// ─── tools/list gating ──────────────────────────────────────────────────────────

/// With a def replacing `Shell` and a session-tool transport, `tools/list` advertises the three
/// session-action tools and NOT `Shell` — replacing Shell IS the opt-in; no mode flag exists.
#[tokio::test]
async fn a_shell_replacing_def_swaps_shell_for_the_action_tools() {
    // Given — a transport is configured (the socket needn't answer for tools/list)
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("unused.sock");
    let defs = author_def_json("http://127.0.0.1:1");
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENTS_JSON", &defs),
        ("TDDY_SANDBOX_TOOL_IPC", socket.to_str().unwrap()),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let names = tools_list_names(&mut stdin, &mut stdout).await;
    let _ = child.kill().await;

    // Then
    for tool in ["request_action", "list_actions", "invoke_action"] {
        assert!(
            names.contains(&tool.to_string()),
            "no-bash tools/list must advertise '{tool}'; got: {names:?}"
        );
    }
    assert!(
        !names.contains(&"Shell".to_string()),
        "no-bash tools/list must NOT advertise Shell; got: {names:?}"
    );
    for kept in ["Read", "Write", "Grep", "Glob"] {
        assert!(
            names.contains(&kept.to_string()),
            "no-bash must keep '{kept}'; got: {names:?}"
        );
    }
}

/// Without a Shell-replacing def, the action tools are absent and `Shell` is advertised as today.
#[tokio::test]
async fn default_tools_list_keeps_shell_and_omits_the_action_tools() {
    // Given
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("unused.sock");
    let mut child = spawn_mcp_server(&[("TDDY_SANDBOX_TOOL_IPC", socket.to_str().unwrap())]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let names = tools_list_names(&mut stdin, &mut stdout).await;
    let _ = child.kill().await;

    // Then
    assert!(names.contains(&"Shell".to_string()));
    for tool in ["request_action", "list_actions", "invoke_action"] {
        assert!(
            !names.contains(&tool.to_string()),
            "default tools/list must NOT advertise '{tool}'; got: {names:?}"
        );
    }
}

/// A def replacing the write tools removes them from the catalog while `Shell` stays — every
/// replacement is independent and per-tool.
#[tokio::test]
async fn a_write_replacing_def_drops_mutation_tools_but_keeps_shell() {
    // Given — a coder def replacing the three mutation tools
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("unused.sock");
    let defs = json!([{
        "name": "coder",
        "model": "some-coder-model",
        "base_url": "http://127.0.0.1:1",
        "replaces": ["Write", "StrReplace", "Delete"],
        "tools": ["READ", "GLOB", "GREP", "WRITE", "STR_REPLACE", "DELETE"],
    }])
    .to_string();
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENTS_JSON", &defs),
        ("TDDY_SANDBOX_TOOL_IPC", socket.to_str().unwrap()),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let names = tools_list_names(&mut stdin, &mut stdout).await;
    let _ = child.kill().await;

    // Then
    for tool in ["Write", "StrReplace", "Delete"] {
        assert!(
            !names.contains(&tool.to_string()),
            "no-write tools/list must NOT advertise '{tool}'; got: {names:?}"
        );
    }
    for kept in ["Shell", "Read", "Grep"] {
        assert!(
            names.contains(&kept.to_string()),
            "no-write must keep '{kept}'; got: {names:?}"
        );
    }
}

// ─── request_action flow ────────────────────────────────────────────────────────

/// The full no-bash authoring loop: `request_action` prompts the configured author model, the
/// authored manifest passes pre-validation, and an `EstablishAction` dispatch carries the YAML
/// to the host relay — whose summary JSON becomes the tool result.
#[tokio::test]
async fn request_action_establishes_an_authored_manifest_via_the_host_relay() {
    // Given — an author model that answers with a valid manifest, and a fake host relay
    let author_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response(VALID_MANIFEST)),
        )
        .mount(&author_server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (socket_path, recorded) = start_fake_host_relay(dir.path());

    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENTS_JSON", &author_def_json(&author_server.uri())),
        ("TDDY_SANDBOX_TOOL_IPC", socket_path.to_str().unwrap()),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        1,
        "request_action",
        json!({"description": "run the tddy-core test suite", "suggested_id": "run-core-tests"}),
    )
    .await;
    let _ = child.kill().await;

    // Then — the tool result is the host's establish summary
    let body = tool_result_json(&response);
    assert_eq!(body["id"].as_str(), Some("run-core-tests"), "got: {body}");

    // And — exactly one EstablishAction dispatch reached the host, carrying the authored YAML
    let dispatches = recorded.lock().unwrap();
    let establishes: Vec<_> = dispatches
        .iter()
        .filter(|(name, _)| name == "EstablishAction")
        .collect();
    assert_eq!(
        establishes.len(),
        1,
        "exactly one EstablishAction dispatch expected; got: {dispatches:?}"
    );
    let args: Value = serde_json::from_str(&establishes[0].1).expect("args must be JSON");
    let yaml = args["yaml"].as_str().expect("args must carry yaml");
    let manifest =
        tddy_core::session_actions::parse_action_manifest_yaml(yaml).expect("yaml must parse");
    assert_eq!(manifest.id, "run-core-tests");
    assert_eq!(manifest.command[0], "cargo");
}

/// An invalid first answer is corrected through the bounded retry loop: the validation error
/// goes back to the author as the next turn, and the second (valid) answer is established.
#[tokio::test]
async fn request_action_retries_after_an_invalid_manifest_and_then_establishes() {
    // Given — the author answers garbage once, then a valid manifest
    let author_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(final_answer_response("this is not yaml: [unbalanced")),
        )
        .up_to_n_times(1)
        .mount(&author_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response(VALID_MANIFEST)),
        )
        .mount(&author_server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (socket_path, recorded) = start_fake_host_relay(dir.path());

    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENTS_JSON", &author_def_json(&author_server.uri())),
        ("TDDY_SANDBOX_TOOL_IPC", socket_path.to_str().unwrap()),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        1,
        "request_action",
        json!({"description": "run the tddy-core test suite"}),
    )
    .await;
    let _ = child.kill().await;

    // Then — established despite the bad first attempt
    let body = tool_result_json(&response);
    assert_eq!(body["id"].as_str(), Some("run-core-tests"), "got: {body}");

    // And — the model was called twice (initial + one correction turn)
    assert_eq!(
        author_server.received_requests().await.unwrap().len(),
        2,
        "the author must get exactly one correction turn"
    );
    assert_eq!(
        recorded
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == "EstablishAction")
            .count(),
        1
    );
}

/// Without a Shell-replacing def, `request_action` is not registered at all — calling it fails
/// at the JSON-RPC layer and nothing is dispatched to the host. Replacing `Shell` is the sole
/// opt-in for the session-action surface.
#[tokio::test]
async fn request_action_without_a_shell_replacing_def_is_not_available() {
    // Given — subagents configured, but none replaces Shell
    let dir = tempfile::tempdir().unwrap();
    let (socket_path, recorded) = start_fake_host_relay(dir.path());
    let defs = json!([{
        "name": "fastcontext",
        "model": "m",
        "base_url": "http://127.0.0.1:1",
        "replaces": ["Grep", "Glob"],
    }])
    .to_string();
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENTS_JSON", &defs),
        ("TDDY_SANDBOX_TOOL_IPC", socket_path.to_str().unwrap()),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When — not advertised, and a direct call fails at the JSON-RPC layer
    let names = tools_list_names(&mut stdin, &mut stdout).await;
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "request_action",
        json!({"description": "run tests"}),
    )
    .await;
    let _ = child.kill().await;

    // Then
    assert!(
        !names.contains(&"request_action".to_string()),
        "request_action must not be advertised without a Shell replacer; got: {names:?}"
    );
    assert!(
        response.get("error").is_some()
            || response["result"]["isError"].as_bool() == Some(true),
        "calling the unregistered tool must fail; got: {response}"
    );
    assert!(
        recorded.lock().unwrap().is_empty(),
        "nothing must be dispatched without an author"
    );
}
