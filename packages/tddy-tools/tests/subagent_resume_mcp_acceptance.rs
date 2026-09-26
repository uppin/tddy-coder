//! Acceptance tests: `subagent_resume`, a per-call `maxTurns`, and the message list every turn
//! outcome now carries — over the real `tddy-tools --mcp` stdio wire.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md
//! PRD: docs/ft/coder/1-WIP/PRD-2026-09-26-subagent-turn-control-and-honest-tool-failure.md
//! Changeset: docs/dev/1-WIP/2026-09-26-subagent-turn-control-and-honest-tool-failure.md
//!
//! Spawns the actual compiled binary and speaks the newline-delimited JSON-RPC wire, the same
//! seam Claude Code talks to — because the thing under test here is the *advertised contract*, and
//! calling `PermissionServer` methods directly would prove nothing about what a main agent can
//! see or send. Mirrors `subagent_mcp_acceptance.rs`.
//!
//! What these tests cannot reach: the model's own view of the conversation. A rewind is only
//! observable in what the provider receives, and that is asserted in
//! `packages/tddy-discovery/tests/subagent_resume_red.rs`. Here the question is narrower and
//! still necessary — can a main agent discover the tool, name a message, and be told when its
//! request was reshaped.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// The ceiling a caller's `maxTurns` is clamped to. Mirrors
/// `tddy_discovery::subagent::SUBAGENT_MAX_TURNS_CEILING`; stated here as a literal so a change to
/// the constant has to be made deliberately on both sides of the wire.
const THE_CEILING: u64 = 50;

/// A tool result far larger than any sensible preview — the size a single `Read` reached in the
/// 2026-09-26 session.
const A_HUGE_ANSWER: usize = 42_000;

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

fn a_turn_that_reads() -> Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Reading.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "READ", "arguments": json!({"path": "src/a.rs"}).to_string()}
                }]
            },
            "finish_reason": "tool_calls"
        }]
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

async fn tools_list(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>) -> Vec<Value> {
    send_json_line(
        stdin,
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
    )
    .await;
    let response = read_json_line(stdout).await;
    response["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list must return a tools array, got: {response}"))
        .clone()
}

/// A conversation open against `explorer`, with the wire held open for further calls.
struct AnOpenConversation {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    session_id: String,
}

impl AnOpenConversation {
    async fn over(base_url: &str) -> Self {
        let mut child = spawn_mcp_server(&[("TDDY_SUBAGENTS_JSON", &explorer_def_json(base_url))]);
        let mut stdin = child.stdin.take().expect("child stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        initialize_mcp_session(&mut stdin, &mut stdout).await;
        let opened = call_tool(
            &mut stdin,
            &mut stdout,
            1,
            "subagent_new_session",
            json!({"agent": "explorer", "sessionId": "conv-resume"}),
        )
        .await;
        let session_id = tool_result_json(&opened)["sessionId"]
            .as_str()
            .expect("an opened conversation has an id")
            .to_string();
        Self {
            child,
            stdin,
            stdout,
            session_id,
        }
    }

    async fn call(&mut self, id: i64, tool: &str, mut arguments: Value) -> Value {
        arguments["sessionId"] = json!(self.session_id);
        let response = call_tool(&mut self.stdin, &mut self.stdout, id, tool, arguments).await;
        tool_result_json(&response)
    }

    async fn close(mut self) {
        let _ = self.child.kill().await;
    }
}

async fn a_model_that_answers_at_once() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response("src/a.rs:1-9")),
        )
        .mount(&server)
        .await;
    server
}

// ─── Advertisement ────────────────────────────────────────────────────────────

/// A tool a main agent cannot see is a tool it cannot use. `subagent_resume` follows the same
/// roster gating as every other conversation tool.
#[tokio::test]
async fn subagent_resume_is_advertised_when_an_agent_is_attached_and_withheld_when_none_is() {
    // Given a server with an agent on its roster, and one without
    let mut with_agent = spawn_mcp_server(&[(
        "TDDY_SUBAGENTS_JSON",
        &explorer_def_json("http://127.0.0.1:1"),
    )]);
    let mut with_stdin = with_agent.stdin.take().expect("child stdin");
    let mut with_stdout = BufReader::new(with_agent.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut with_stdin, &mut with_stdout).await;

    let mut without_agent = spawn_mcp_server(&[]);
    let mut without_stdin = without_agent.stdin.take().expect("child stdin");
    let mut without_stdout = BufReader::new(without_agent.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut without_stdin, &mut without_stdout).await;

    // When each is asked what it offers
    let offered = tools_list(&mut with_stdin, &mut with_stdout).await;
    let withheld = tools_list(&mut without_stdin, &mut without_stdout).await;
    let _ = with_agent.kill().await;
    let _ = without_agent.kill().await;

    // Then the resume tool appears exactly where the rest of the conversation surface does
    let names = |tools: &[Value]| -> Vec<String> {
        tools
            .iter()
            .map(|t| t["name"].as_str().unwrap_or_default().to_string())
            .collect()
    };
    let offered_names = names(&offered);
    let withheld_names = names(&withheld);
    assert!(
        offered_names.contains(&"subagent_resume".to_string()),
        "a session with an agent must be able to resume a conversation; got: {offered_names:?}"
    );
    assert!(
        !withheld_names.contains(&"subagent_resume".to_string()),
        "a session with no agent must not be offered a conversation tool; got: {withheld_names:?}"
    );

    // And its schema names the two things that make it more than a retry
    let schema = offered
        .iter()
        .find(|t| t["name"] == "subagent_resume")
        .expect("the resume tool")["inputSchema"]
        .clone();
    let properties = &schema["properties"];
    for field in ["sessionId", "fromMessageId", "correction", "maxTurns"] {
        assert!(
            !properties[field].is_null(),
            "subagent_resume must accept '{field}'; got schema: {schema}"
        );
    }
}

// ─── Message descriptors on the wire ──────────────────────────────────────────

/// The field whose absence let a main agent read 54 refused tool calls as progress.
#[tokio::test]
async fn a_turn_outcome_lists_the_messages_the_turn_appended() {
    // Given a conversation whose agent answers at once
    let server = a_model_that_answers_at_once().await;
    let mut conversation = AnOpenConversation::over(&server.uri()).await;

    // When it is prompted
    let outcome = conversation
        .call(
            2,
            "subagent_prompt",
            json!({"prompt": [{"type": "text", "text": "where is auth?"}]}),
        )
        .await;
    conversation.close().await;

    // Then the outcome accounts for the exchange, with an addressable id per message
    let messages = outcome["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("a turn outcome must list its messages; got: {outcome}"))
        .clone();
    assert!(
        !messages.is_empty(),
        "a turn that appended messages must report them; got: {outcome}"
    );
    for message in &messages {
        assert!(
            message["id"].as_str().is_some_and(|id| !id.is_empty()),
            "every message needs an id a resume can name; got: {message}"
        );
        assert!(
            message["role"].as_str().is_some(),
            "every message needs a role; got: {message}"
        );
    }
}

/// A preview is a handle, not a copy. Returning whole tool payloads in every outcome would put
/// the subagent's context back into the main agent's, which is the cost the subagent exists to
/// avoid.
#[tokio::test]
async fn a_large_tool_result_is_previewed_rather_than_returned_whole() {
    // Given a model that reads a very large file and then answers
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_turn_that_reads()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(final_answer_response("src/a.rs:1-9")),
        )
        .mount(&server)
        .await;

    let worktree = tempfile::tempdir().expect("a worktree");
    std::fs::write(worktree.path().join("a.rs"), "x".repeat(A_HUGE_ANSWER)).expect("a big file");

    let mut conversation = AnOpenConversation::over(&server.uri()).await;

    // When the agent reads it
    let outcome = conversation
        .call(
            2,
            "subagent_prompt",
            json!({"prompt": [{"type": "text", "text": "read a.rs"}]}),
        )
        .await;
    conversation.close().await;

    // Then no descriptor carries anything like the payload
    let messages = outcome["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("a turn outcome must list its messages; got: {outcome}"))
        .clone();
    for message in &messages {
        let preview = message["preview"].as_str().unwrap_or_default();
        assert!(
            preview.chars().count() < A_HUGE_ANSWER / 10,
            "a {}-character preview is a copy of the payload, not a handle",
            preview.chars().count()
        );
    }
}

// ─── Turn budget over the wire ────────────────────────────────────────────────

/// A clamped budget is reported, because a caller given fewer turns than it asked for otherwise
/// reads an early stop as a finished search.
#[tokio::test]
async fn a_max_turns_above_the_ceiling_is_clamped_and_the_outcome_says_so() {
    // Given a conversation
    let server = a_model_that_answers_at_once().await;
    let mut conversation = AnOpenConversation::over(&server.uri()).await;

    // When it is prompted with an absurd budget
    let outcome = conversation
        .call(
            2,
            "subagent_prompt",
            json!({
                "prompt": [{"type": "text", "text": "trace every caller"}],
                "maxTurns": THE_CEILING * 10,
            }),
        )
        .await;
    conversation.close().await;

    // Then the answer says what budget actually applied
    assert_eq!(
        outcome["clampedMaxTurns"].as_u64(),
        Some(THE_CEILING),
        "an over-large budget must be clamped and reported; got: {outcome}"
    );
}

// ─── Refusals ─────────────────────────────────────────────────────────────────

/// Resuming a conversation that does not exist is a mistake worth naming, not a silent no-op.
#[tokio::test]
async fn resuming_an_unknown_conversation_is_an_error_result() {
    // Given a server with an agent but no open conversation
    let mut child = spawn_mcp_server(&[(
        "TDDY_SUBAGENTS_JSON",
        &explorer_def_json("http://127.0.0.1:1"),
    )]);
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    initialize_mcp_session(&mut stdin, &mut stdout).await;

    // When a resume names a conversation that was never opened
    let response = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "subagent_resume",
        json!({"sessionId": "conv-never-opened"}),
    )
    .await;
    let _ = child.kill().await;

    // Then it comes back as an in-band error result naming the id
    let body = tool_result_json(&response);
    let message = body["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("conv-never-opened"),
        "the refusal must name the conversation the caller got wrong; got: {body}"
    );
}
