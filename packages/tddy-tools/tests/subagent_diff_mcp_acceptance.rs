//! Acceptance tests: `subagent_diff` over the real `tddy-tools --mcp` stdio wire.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-30-agent-worktree-diff.md
//!
//! What these tests cannot reach: a diff with content. With no session-tool transport configured the
//! subagent reads through `CodebaseAccess::Local`, which refuses every write, so no conversation here
//! has commits; the diff itself is pinned in
//! `packages/tddy-subagent-worktree/tests/diff_acceptance.rs` and
//! `packages/tddy-daemon-rpc/tests/conversation_worktree_diff_acceptance.rs`.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const IO_TIMEOUT: Duration = Duration::from_secs(10);
const CONVERSATION: &str = "conv-diff";

fn explorer_def_json(base_url: &str) -> String {
    json!([{
        "name": "explorer",
        "model": "qwen2.5-coder:7b",
        "base_url": base_url,
        "tools": ["READ", "GLOB", "GREP"],
        "max_turns": 3,
        "replaces": []
    }])
    .to_string()
}

fn a_final_answer() -> Value {
    json!({
        "choices": [{
            "message": { "role": "assistant", "content": "<final_answer>\nsrc/a.rs:1\n</final_answer>" },
            "finish_reason": "stop"
        }]
    })
}

/// A model that takes `after` to answer — long enough for a turn to still be running when the
/// test makes its next call.
async fn a_model_that_answers_after(after: Duration) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(a_final_answer())
                .set_delay(after),
        )
        .mount(&server)
        .await;
    server
}

/// A conversation open against `explorer`, with the wire held open for further calls.
struct AnOpenConversation {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl AnOpenConversation {
    async fn over(base_url: &str) -> Self {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
            .arg("--mcp")
            .env("TDDY_SUBAGENTS_JSON", explorer_def_json(base_url))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn tddy-tools --mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let mut conversation = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        conversation.initialize().await;
        conversation
            .call(
                "subagent_new_session",
                json!({ "agent": "explorer", "sessionId": CONVERSATION }),
            )
            .await;
        conversation
    }

    async fn initialize(&mut self) {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tddy-subagent-diff-test", "version": "0.0.1"}
            }
        }))
        .await;
        self.receive().await;
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
    }

    /// Call `tool` and return the JSON its text block carries.
    async fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments}
        }))
        .await;
        let response = self.receive().await;
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("tools/call must answer a text block; got: {response}"))
            .to_string();
        serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("tool result text {text:?} was not JSON: {e}"))
    }

    async fn advertised_tool_names(&mut self) -> Vec<String> {
        self.send(json!({"jsonrpc": "2.0", "id": 9_999, "method": "tools/list", "params": {}}))
            .await;
        let response = self.receive().await;
        response["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("tools/list must return a tools array, got: {response}"))
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect()
    }

    async fn diff(&mut self, session_id: &str) -> Value {
        self.call("subagent_diff", json!({ "sessionId": session_id }))
            .await
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        tokio::time::timeout(IO_TIMEOUT, self.stdin.write_all(line.as_bytes()))
            .await
            .expect("write to tddy-tools stdin timed out")
            .expect("write to tddy-tools stdin");
    }

    async fn receive(&mut self) -> Value {
        let mut line = String::new();
        tokio::time::timeout(IO_TIMEOUT, self.stdout.read_line(&mut line))
            .await
            .expect("read from tddy-tools stdout timed out")
            .expect("read from tddy-tools stdout");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"))
    }

    async fn close(mut self) {
        let _ = self.child.kill().await;
    }
}

#[tokio::test]
async fn subagent_diff_is_advertised() {
    // Given
    let model = a_model_that_answers_after(Duration::ZERO).await;
    let mut conversation = AnOpenConversation::over(&model.uri()).await;

    // When
    let advertised = conversation.advertised_tool_names().await;

    // Then
    assert!(advertised.contains(&"subagent_diff".to_string()));
    conversation.close().await;
}

#[tokio::test]
async fn a_diff_of_a_conversation_that_never_wrote_is_refused() {
    // Given
    let model = a_model_that_answers_after(Duration::ZERO).await;
    let mut conversation = AnOpenConversation::over(&model.uri()).await;

    // When
    let refused = conversation.diff(CONVERSATION).await;

    // Then
    assert_eq!(refused["is_error"], json!(true));
    conversation.close().await;
}

#[tokio::test]
async fn a_diff_of_an_unknown_conversation_is_refused() {
    // Given
    let model = a_model_that_answers_after(Duration::ZERO).await;
    let mut conversation = AnOpenConversation::over(&model.uri()).await;

    // When
    let refused = conversation.diff("no-such-conversation").await;

    // Then
    assert_eq!(refused["is_error"], json!(true));
    conversation.close().await;
}
