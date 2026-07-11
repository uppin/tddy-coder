//! Acceptance tests: per-conversation token accounting exposed over the real `tddy-tools --mcp`
//! stdio wire — the `subagent_list` listing tool, per-turn usage on `subagent_prompt` results, and
//! the host-visible accounting file.
//!
//! Feature: docs/ft/coder/session-token-accounting.md (requirement 1, acceptance criterion 1)
//! Changeset: docs/dev/1-WIP/2026-07-11-changeset-session-token-accounting.md
//!
//! Mirrors `subagent_mcp_acceptance.rs`: spawn the actual compiled `tddy-tools` binary and speak
//! the real newline-delimited JSON-RPC wire (the exact seam Claude Code talks to) so the listing
//! tool and accounting are exercised end-to-end, not via internal calls.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// A final-answer response that also reports token usage, as both FastContext endpoints and Ollama
/// do on `/v1/chat/completions`.
fn final_answer_with_usage(answer: &str, prompt_tokens: u64, completion_tokens: u64) -> Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("Looked at the code.\n<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": prompt_tokens + completion_tokens
        }
    })
}

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
                "clientInfo": {"name": "tddy-token-test-client", "version": "0.0.1"}
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
        .unwrap_or_else(|| panic!("tools/call result must carry a text content block; got: {response}"));
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tool result text {text:?} was not valid JSON: {e}"))
}

/// Find the single conversation with the given id in a `subagent_list` / accounting payload.
fn conversation<'a>(body: &'a Value, id: &str) -> &'a Value {
    body["conversations"]
        .as_array()
        .unwrap_or_else(|| panic!("payload must carry a conversations array; got: {body}"))
        .iter()
        .find(|c| c["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("no conversation with id {id:?} in payload; got: {body}"))
}

async fn open_and_prompt(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    base_id: i64,
    session_id: &str,
    prompts: &[&str],
) {
    call_tool(stdin, stdout, base_id, "subagent_new_session", json!({"sessionId": session_id})).await;
    for (i, text) in prompts.iter().enumerate() {
        call_tool(
            stdin,
            stdout,
            base_id + 1 + i as i64,
            "subagent_prompt",
            json!({"sessionId": session_id, "prompt": [{"type": "text", "text": text}]}),
        )
        .await;
    }
}

// ─── subagent_list ─────────────────────────────────────────────────────────────

/// The headline behavior: a session that opened two subagent conversations can list both, each
/// with its exact cumulative input/output/total tokens, turn count, agent name, and model.
#[tokio::test]
async fn subagent_list_reports_each_open_conversation_with_its_cumulative_token_totals() {
    // Given — every model turn reports 100 prompt + 40 completion tokens.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_with_usage("src/a.rs:1-1", 100, 40)))
        .mount(&server)
        .await;

    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "fastcontext"),
        ("TDDY_SUBAGENT_FASTCONTEXT_URL", server.uri().as_str()),
        ("TDDY_SUBAGENT_FASTCONTEXT_MODEL", "test-model"),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // conv-a is prompted twice, conv-b once.
    open_and_prompt(&mut stdin, &mut stdout, 10, "conv-a", &["q1", "q2"]).await;
    open_and_prompt(&mut stdin, &mut stdout, 20, "conv-b", &["q1"]).await;

    // When
    let response = call_tool(&mut stdin, &mut stdout, 30, "subagent_list", json!({})).await;
    let _ = child.kill().await;

    // Then
    let body = tool_result_json(&response);

    let a = conversation(&body, "conv-a");
    assert_eq!(a["agent"].as_str(), Some("fastcontext"), "conv-a agent; got: {a}");
    assert_eq!(a["model"].as_str(), Some("test-model"), "conv-a model; got: {a}");
    assert_eq!(a["inputTokens"].as_u64(), Some(200), "conv-a input; got: {a}");
    assert_eq!(a["outputTokens"].as_u64(), Some(80), "conv-a output; got: {a}");
    assert_eq!(a["totalTokens"].as_u64(), Some(280), "conv-a total; got: {a}");
    assert_eq!(a["turns"].as_u64(), Some(2), "conv-a turns; got: {a}");

    let b = conversation(&body, "conv-b");
    assert_eq!(b["inputTokens"].as_u64(), Some(100), "conv-b input; got: {b}");
    assert_eq!(b["outputTokens"].as_u64(), Some(40), "conv-b output; got: {b}");
    assert_eq!(b["totalTokens"].as_u64(), Some(140), "conv-b total; got: {b}");
    assert_eq!(b["turns"].as_u64(), Some(1), "conv-b turns; got: {b}");
}

// ─── per-turn usage on the prompt result ───────────────────────────────────────

/// A `subagent_prompt` result carries the token usage of the turn it just ran.
#[tokio::test]
async fn subagent_prompt_result_includes_the_turns_token_usage() {
    // Given
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_with_usage("src/a.rs:1-1", 100, 40)))
        .mount(&server)
        .await;
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "fastcontext"),
        ("TDDY_SUBAGENT_FASTCONTEXT_URL", server.uri().as_str()),
        ("TDDY_SUBAGENT_FASTCONTEXT_MODEL", "test-model"),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;
    call_tool(&mut stdin, &mut stdout, 1, "subagent_new_session", json!({"sessionId": "conv-x"})).await;

    // When
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "subagent_prompt",
        json!({"sessionId": "conv-x", "prompt": [{"type": "text", "text": "Where is auth?"}]}),
    )
    .await;
    let _ = child.kill().await;

    // Then
    let body = tool_result_json(&response);
    assert_eq!(body["usage"]["inputTokens"].as_u64(), Some(100), "prompt usage input; got: {body}");
    assert_eq!(body["usage"]["outputTokens"].as_u64(), Some(40), "prompt usage output; got: {body}");
    assert_eq!(body["usage"]["totalTokens"].as_u64(), Some(140), "prompt usage total; got: {body}");
}

// ─── host-visible accounting file ──────────────────────────────────────────────

/// When `TDDY_TOOLS_ACCOUNTING_FILE` is set (the runner points it into the host-visible egress
/// dir), the MCP server writes the full conversation list there so the host `tddy-sandbox-app` can
/// read and print it after the session ends.
#[tokio::test]
async fn writes_the_accounting_file_with_all_conversations_when_the_env_var_is_set() {
    // Given
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_with_usage("src/a.rs:1-1", 100, 40)))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let accounting_path = dir.path().join("accounting.json");
    let accounting_str = accounting_path.to_str().expect("utf-8 path");

    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "fastcontext"),
        ("TDDY_SUBAGENT_FASTCONTEXT_URL", server.uri().as_str()),
        ("TDDY_SUBAGENT_FASTCONTEXT_MODEL", "test-model"),
        ("TDDY_TOOLS_ACCOUNTING_FILE", accounting_str),
    ]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When
    open_and_prompt(&mut stdin, &mut stdout, 1, "conv-1", &["Where is auth?"]).await;
    let _ = child.kill().await;

    // Then — the accounting file records conv-1 with its totals.
    let contents = std::fs::read_to_string(&accounting_path)
        .unwrap_or_else(|e| panic!("accounting file {accounting_str} must exist: {e}"));
    let body: Value = serde_json::from_str(&contents)
        .unwrap_or_else(|e| panic!("accounting file must be valid JSON: {e}; contents: {contents}"));
    let c = conversation(&body, "conv-1");
    assert_eq!(c["totalTokens"].as_u64(), Some(140), "conv-1 total in accounting file; got: {c}");
    assert_eq!(c["turns"].as_u64(), Some(1), "conv-1 turns in accounting file; got: {c}");
}
