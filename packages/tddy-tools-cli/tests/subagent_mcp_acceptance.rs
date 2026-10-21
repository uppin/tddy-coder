//! Acceptance tests: ACP-shaped subagent tools (`subagent_new_session`, `subagent_prompt`,
//! `subagent_cancel`) exposed over the real `tddy-tools --mcp` stdio wire.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (criteria 6-9)
//! Changeset: docs/dev/1-WIP/2026-07-01-changeset-managed-codebase-subagents.md
//!
//! Mirrors `mcp_stdio_dynamic_tools_acceptance.rs`'s approach: spawn the actual compiled
//! `tddy-tools` binary and speak the real newline-delimited JSON-RPC wire (the exact framing
//! `rmcp::transport::stdio()` uses, and the same seam Claude Code talks to) rather than calling
//! `PermissionServer` methods directly — this is the seam that must actually advertise and
//! dispatch the subagent tools end-to-end.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// `TDDY_SUBAGENTS_JSON` holding one def named `explorer` pointed at `base_url` — the shape the
/// daemon injects into the jail, and the only source of a subagent's configuration.
fn explorer_def_json(base_url: &str) -> String {
    json!([{
        "name": "explorer",
        "model": "qwen2.5-coder:7b",
        "base_url": base_url,
        "tools": ["READ", "GLOB", "GREP"],
        "max_turns": 6,
        "replaces": []
    }])
    .to_string()
}

fn final_answer_response(answer: &str) -> Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("Looked at the code.\n<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }]
    })
}

/// Spawn the real `tddy-tools --mcp` binary with the given extra env vars.
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

/// Perform the MCP `initialize` handshake (request + `notifications/initialized`).
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

/// Extracts and parses the first text content block of a `tools/call` result as JSON.
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

// ─── AC6: the roster decides whether the conversation tools are advertised ─────

/// With an agent on the session's roster — here the spawn seed, before any frame — `tools/list`
/// advertises all three ACP-shaped subagent tools. `TDDY_SUBAGENT` is deliberately not set: the
/// roster is what decides, and a session's agents are no longer a spawn-time env var.
#[tokio::test]
async fn tools_list_advertises_the_subagent_tools_when_an_agent_is_attached() {
    // Given
    let mut child = spawn_mcp_server(&[(
        "TDDY_SUBAGENTS_JSON",
        &explorer_def_json("http://127.0.0.1:1"),
    )]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let names = tools_list_names(&mut stdin, &mut stdout).await;
    let _ = child.kill().await;

    // Then
    for tool in ["subagent_new_session", "subagent_prompt", "subagent_cancel"] {
        assert!(
            names.contains(&tool.to_string()),
            "tools/list must advertise '{tool}' while an agent is attached; got: {names:?}"
        );
    }
}

/// With no agent attached, none of the three subagent tools is advertised — there is nobody behind
/// them to open a conversation with, and four tools that can only answer "no agents are attached"
/// are noise in the main agent's tool list.
#[tokio::test]
async fn tools_list_omits_the_subagent_tools_when_no_agent_is_attached() {
    // Given
    let mut child = spawn_mcp_server(&[]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let names = tools_list_names(&mut stdin, &mut stdout).await;
    let _ = child.kill().await;

    // Then
    for tool in ["subagent_new_session", "subagent_prompt", "subagent_cancel"] {
        assert!(
            !names.contains(&tool.to_string()),
            "tools/list must NOT advertise '{tool}' with an empty roster; got: {names:?}"
        );
    }
}

// ─── AC7: caller-supplied session id ───────────────────────────────────────────

/// `subagent_new_session` must use the caller-supplied `sessionId` verbatim — the main agent, not
/// the subagent server, decides the conversation id.
#[tokio::test]
async fn subagent_new_session_honors_a_caller_supplied_session_id() {
    // Given
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "explorer"),
        (
            "TDDY_SUBAGENTS_JSON",
            &explorer_def_json("http://127.0.0.1:1"),
        ),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        1,
        "subagent_new_session",
        json!({"agent": "explorer", "sessionId": "conv-42"}),
    )
    .await;
    let _ = child.kill().await;

    // Then
    let body = tool_result_json(&response);
    assert_eq!(
        body["sessionId"].as_str(),
        Some("conv-42"),
        "subagent_new_session must echo back the caller-supplied sessionId verbatim; got: {body}"
    );
}

// ─── AC8: ping-pong prompt continues the same conversation ─────────────────────

/// Two `subagent_prompt` calls against the same `sessionId` must (a) both yield with
/// `stopReason:"end_turn"` and citation content, and (b) be sent to the model as one growing
/// conversation — proving the session created by `subagent_new_session` is actually reused across
/// separate MCP `tools/call` invocations, not reconstructed fresh each time.
#[tokio::test]
async fn subagent_prompt_ping_pongs_against_the_same_session_and_retains_history() {
    // Given — the mock endpoint always answers immediately
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response("src/auth.rs:1-50")),
        )
        .mount(&server)
        .await;

    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "explorer"),
        ("TDDY_SUBAGENTS_JSON", &explorer_def_json(&server.uri())),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    call_tool(
        &mut stdin,
        &mut stdout,
        1,
        "subagent_new_session",
        json!({"agent": "explorer", "sessionId": "conv-77"}),
    )
    .await;

    // When — two prompts against the same session id
    let first = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "subagent_prompt",
        json!({"sessionId": "conv-77", "prompt": [{"type": "text", "text": "Where is the authentication logic?"}]}),
    )
    .await;
    let second = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "subagent_prompt",
        json!({"sessionId": "conv-77", "prompt": [{"type": "text", "text": "Is there rate limiting there too?"}]}),
    )
    .await;
    let _ = child.kill().await;

    // Then — both calls yielded end_turn with the citation content
    for response in [&first, &second] {
        let body = tool_result_json(response);
        assert_eq!(body["stopReason"].as_str(), Some("end_turn"));
        assert_eq!(
            body["content"][0]["text"].as_str(),
            Some("src/auth.rs:1-50")
        );
    }

    // And — the second model call's message history includes both user prompts, proving the same
    // session (not a fresh one) handled the second `subagent_prompt` call.
    let calls = server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 2, "exactly two model calls must be made");
    let second_body: Value =
        serde_json::from_slice(&calls[1].body).expect("second request body must be valid JSON");
    let user_texts: Vec<&str> = second_body["messages"]
        .as_array()
        .expect("request body must carry a messages array")
        .iter()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str())
        .collect();
    assert_eq!(
        user_texts,
        vec![
            "Where is the authentication logic?",
            "Is there rate limiting there too?"
        ],
        "second subagent_prompt call must retain the first call's history; got: {user_texts:?}"
    );
}

// ─── The spawn seed must parse, or nothing is served ────────────────────────────

/// A daemon newer than this binary writes a def field it does not know, and `SpecializedAgentDef`
/// is `deny_unknown_fields` — so the seed does not parse. Serving on would mean an empty roster:
/// no agent attached, and every tool the session's agents took over handed straight back to the
/// main agent, with nothing naming the cause. The server refuses to start instead, saying which
/// variable it could not read.
#[tokio::test]
async fn refuses_to_start_when_the_spawn_seed_carries_a_field_this_build_does_not_know() {
    // Given — the def a newer daemon serialized, carrying one field this build predates
    let seed_from_a_newer_daemon = json!([{
        "name": "explorer",
        "model": "qwen2.5-coder:7b",
        "base_url": "http://127.0.0.1:1",
        "tools": ["READ"],
        "max_turns": 6,
        "replaces": ["Grep"],
        "a_field_this_build_predates": "value"
    }])
    .to_string();

    // When
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .arg("--mcp")
        .env("TDDY_SUBAGENT", "explorer")
        .env("TDDY_SUBAGENTS_JSON", &seed_from_a_newer_daemon)
        .stdin(Stdio::null())
        .output()
        .await
        .expect("run tddy-tools --mcp");

    // Then
    assert!(
        !output.status.success(),
        "an unreadable spawn seed must fail the start; exit status was {}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("TDDY_SUBAGENTS_JSON"),
        "the failure must name the variable it could not read, was: {stderr}"
    );
}

// ─── AC9: unknown session id ────────────────────────────────────────────────────

/// `subagent_prompt` against a `sessionId` that was never opened via `subagent_new_session` must
/// return an error result, not silently create a new session and not panic.
#[tokio::test]
async fn subagent_prompt_against_an_unknown_session_id_returns_an_error_result() {
    // Given
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "explorer"),
        (
            "TDDY_SUBAGENTS_JSON",
            &explorer_def_json("http://127.0.0.1:1"),
        ),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        1,
        "subagent_prompt",
        json!({"sessionId": "does-not-exist", "prompt": [{"type": "text", "text": "hello"}]}),
    )
    .await;
    let _ = child.kill().await;

    // Then
    let body = tool_result_json(&response);
    assert_eq!(
        body["is_error"].as_bool(),
        Some(true),
        "subagent_prompt against an unknown sessionId must return is_error:true; got: {body}"
    );
}
