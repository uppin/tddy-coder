//! Acceptance tests: multiple YAML-defined specialized subagents (`TDDY_SUBAGENTS_JSON`) over the
//! real `tddy-tools --mcp` stdio wire — generalizing the single hardcoded `"fastcontext"` factory
//! `subagent_mcp_acceptance.rs` covers.
//!
//! Feature: docs/ft/coder/specialized-subagents.md (criteria 9-10)
//! Changeset: docs/dev/1-WIP/specialized-subagents.md
//!
//! Mirrors `subagent_mcp_acceptance.rs`'s helpers exactly (each acceptance test file in this
//! package is self-contained — see `packages/tddy-tools/tests/`).

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Spawn the real `tddy-tools --mcp` binary with the given extra env vars. Stderr is inherited
/// (not swallowed) — if the process panics (e.g. an unimplemented code path), the panic message
/// appears in this test's own output instead of manifesting only as a mysterious stdout timeout.
fn spawn_mcp_server(env: &[(&str, &str)]) -> Child {
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"));
    cmd.arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
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

/// Two independent `SpecializedAgentDef`s, serialized as the `TDDY_SUBAGENTS_JSON` env var —
/// deliberately unreachable base_urls (port 1): these tests only exercise session *creation*
/// (which def gets resolved by name), not a live model round-trip.
fn two_agent_defs_json() -> String {
    json!([
        {
            "name": "agent-one",
            "model": "model-one",
            "base_url": "http://127.0.0.1:1",
            "max_turns": 5
        },
        {
            "name": "agent-two",
            "model": "model-two",
            "base_url": "http://127.0.0.1:1",
            "max_turns": 5
        }
    ])
    .to_string()
}

/// `subagent_new_session { agent: "agent-two" }` must resolve the *second* of two defs configured
/// via `TDDY_SUBAGENTS_JSON` — proving the registry distinguishes multiple specialized agents by
/// name, not just the single hardcoded `"fastcontext"` factory `subagent_mcp_acceptance.rs` covers.
#[tokio::test]
async fn subagent_new_session_selects_the_named_agent_among_multiple_configured_via_json() {
    // Given
    let defs_json = two_agent_defs_json();
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "agent-one,agent-two"),
        ("TDDY_SUBAGENTS_JSON", &defs_json),
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
        json!({"sessionId": "conv-1", "agent": "agent-two"}),
    )
    .await;
    let _ = child.kill().await;

    // Then — session creation for the second configured def must succeed (not "unknown subagent")
    let body = tool_result_json(&response);
    assert_eq!(
        body["sessionId"].as_str(),
        Some("conv-1"),
        "subagent_new_session must resolve 'agent-two' from TDDY_SUBAGENTS_JSON and echo back the \
         caller-supplied sessionId; got: {body}"
    );
}

/// `subagent_new_session` with an `agent` name that is present in neither `TDDY_SUBAGENTS_JSON`
/// nor the legacy hardcoded `"fastcontext"` factory must return `is_error:true`, not silently
/// create a session for a different (wrong) agent.
#[tokio::test]
async fn subagent_new_session_rejects_an_agent_name_not_present_in_tddy_subagents_json() {
    // Given
    let defs_json = two_agent_defs_json();
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "agent-one,agent-two"),
        ("TDDY_SUBAGENTS_JSON", &defs_json),
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
        json!({"sessionId": "conv-2", "agent": "not-a-configured-agent"}),
    )
    .await;
    let _ = child.kill().await;

    // Then
    let body = tool_result_json(&response);
    assert_eq!(
        body["is_error"].as_bool(),
        Some(true),
        "an agent name absent from TDDY_SUBAGENTS_JSON must be rejected, not silently \
         substituted; got: {body}"
    );
}

/// Back-compat: with only `TDDY_SUBAGENT=fastcontext` set (no `TDDY_SUBAGENTS_JSON` at all), the
/// legacy single-agent path from `subagent_mcp_acceptance.rs` must keep working unmodified.
#[tokio::test]
async fn back_compat_tddy_subagent_fastcontext_alone_still_resolves_without_subagents_json() {
    // Given — no TDDY_SUBAGENTS_JSON at all, matching today's shipped (#254) configuration
    let mut child = spawn_mcp_server(&[
        ("TDDY_SUBAGENT", "fastcontext"),
        ("TDDY_SUBAGENT_FASTCONTEXT_URL", "http://127.0.0.1:1"),
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
        json!({"sessionId": "conv-3"}),
    )
    .await;
    let _ = child.kill().await;

    // Then
    let body = tool_result_json(&response);
    assert_eq!(
        body["sessionId"].as_str(),
        Some("conv-3"),
        "TDDY_SUBAGENT=fastcontext alone (no TDDY_SUBAGENTS_JSON) must still resolve via the \
         legacy SubagentRegistry::new() path; got: {body}"
    );
}
