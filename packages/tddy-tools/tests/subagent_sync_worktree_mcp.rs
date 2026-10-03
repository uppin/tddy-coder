//! `subagent_prompt` and `subagent_resume` advertise `syncWorktree` — the caller's opt-out from a
//! turn first taking in its current files — over the real `tddy-tools --mcp` stdio wire.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-03-agent-worktree-caller-sync.md
//!
//! The sync itself is pinned where it can be observed: the order and the notice in
//! `packages/tddy-discovery/tests/caller_sync_acceptance.rs`, the git in
//! `packages/tddy-subagent-worktree/tests/caller_sync_acceptance.rs`.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

async fn send(stdin: &mut ChildStdin, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    tokio::time::timeout(IO_TIMEOUT, stdin.write_all(line.as_bytes()))
        .await
        .expect("write timed out")
        .expect("write");
}

async fn receive(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(IO_TIMEOUT, stdout.read_line(&mut line))
        .await
        .expect("read timed out")
        .expect("read");
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"))
}

/// The `inputSchema` a server with one agent on its roster advertises for `tool`.
async fn the_advertised_schema_of(tool: &str) -> Value {
    let defs = json!([{
        "name": "explorer",
        "model": "qwen2.5-coder:7b",
        "base_url": "http://127.0.0.1:1",
        "tools": ["READ"],
        "max_turns": 3,
        "replaces": []
    }])
    .to_string();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
        .arg("--mcp")
        .env("TDDY_SUBAGENTS_JSON", defs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tddy-tools --mcp");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    send(
        &mut stdin,
        json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tddy-sync-worktree-test", "version": "0.0.1"}
            }
        }),
    )
    .await;
    receive(&mut stdout).await;
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;
    send(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
    )
    .await;
    let listed = receive(&mut stdout).await;
    let _ = child.kill().await;
    listed["result"]["tools"]
        .as_array()
        .expect("a tools array")
        .iter()
        .find(|t| t["name"] == tool)
        .unwrap_or_else(|| panic!("{tool} is advertised"))["inputSchema"]
        .clone()
}

#[tokio::test]
async fn subagent_prompt_advertises_sync_worktree_as_a_boolean() {
    // Given / When
    let schema = the_advertised_schema_of("subagent_prompt").await;

    // Then
    assert_eq!(
        schema["properties"]["syncWorktree"]["type"],
        json!("boolean")
    );
}

#[tokio::test]
async fn subagent_resume_advertises_sync_worktree_as_a_boolean() {
    // Given / When
    let schema = the_advertised_schema_of("subagent_resume").await;

    // Then
    assert_eq!(
        schema["properties"]["syncWorktree"]["type"],
        json!("boolean")
    );
}
