//! Acceptance tests: a subagent turn that outruns its grace period hands back a `responseId`
//! instead of blocking the main agent, and `subagent_await` collects it.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (criteria 25-32, 34)
//!
//! Driven over the real `tddy-tools --mcp` stdio wire against a `wiremock` model endpoint — the
//! same seam `subagent_mcp_acceptance.rs` uses — because what is under test is a *timing* contract
//! between two `tools/call` invocations, and only the real transport can show that the first one
//! returned while the turn behind it was still running.

use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Deliberately longer than `subagent_mcp_acceptance.rs`'s five seconds: tests here wait for a turn
/// that is *known* to be slow, so the read that collects it must outlast the delay it was given.
const IO_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a prompt blocks before it gives up waiting and hands back a `responseId`. Short enough
/// that a delayed turn always outruns it.
const SHORT_GRACE: Duration = Duration::from_millis(300);

/// How long a "slow" model takes to answer. Comfortably longer than [`SHORT_GRACE`] on a loaded
/// machine, and comfortably shorter than [`IO_TIMEOUT`].
const SLOW_TURN: Duration = Duration::from_millis(1_500);

/// A blocking budget no test's model comes close to spending — the value a call passes when it
/// expects to be answered rather than deferred.
const AMPLE: Duration = Duration::from_secs(10);

const PROMPT_TOKENS: u32 = 30;
const COMPLETION_TOKENS: u32 = 12;

// ─── The model the subagent talks to ──────────────────────────────────────────

/// `TDDY_SUBAGENTS_JSON` holding one def named `explorer` pointed at `base_url` — the shape the
/// daemon injects into the jail, and the only source of a subagent's configuration.
fn an_explorer_def(base_url: &str) -> String {
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

/// A completion that ends the turn immediately, with the token usage the accounting reads.
fn a_final_answer(answer: &str) -> Value {
    json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("Looked at the code.\n<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": PROMPT_TOKENS,
            "completion_tokens": COMPLETION_TOKENS,
            "total_tokens": PROMPT_TOKENS + COMPLETION_TOKENS
        }
    })
}

async fn a_model_answering(answer: &str, after: Duration) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(a_final_answer(answer))
                .set_delay(after),
        )
        .mount(&server)
        .await;
    server
}

async fn a_model_failing(after: Duration) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(500)
                .set_body_string("upstream exploded")
                .set_delay(after),
        )
        .mount(&server)
        .await;
    server
}

/// The user turns one model call was sent, in order — how a test reads which conversation history
/// a turn actually ran against.
fn user_turns_of(request_body: &[u8]) -> Vec<String> {
    let body: Value = serde_json::from_slice(request_body).expect("model request must be JSON");
    body["messages"]
        .as_array()
        .expect("model request must carry a messages array")
        .iter()
        .filter(|m| m["role"] == "user")
        .filter_map(|m| m["content"].as_str())
        .map(str::to_string)
        .collect()
}

// ─── The MCP server, as a subagent client drives it ───────────────────────────

/// A live `tddy-tools --mcp` process with one `explorer` agent attached, addressed through the
/// conversation tools.
struct SubagentTools {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl SubagentTools {
    async fn talking_to(model: &MockServer) -> Self {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_tddy-tools"))
            .arg("--mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("TDDY_SUBAGENT", "explorer")
            .env("TDDY_SUBAGENTS_JSON", an_explorer_def(&model.uri()))
            .spawn()
            .expect("spawn tddy-tools --mcp");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let mut tools = Self {
            child,
            stdin,
            stdout,
            next_id: 0,
        };
        tools.initialize().await;
        tools
    }

    async fn initialize(&mut self) {
        let id = self.take_id();
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "tddy-async-subagent-test-client", "version": "0.0.1"}
            }
        }))
        .await;
        self.read().await;
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
    }

    async fn open_conversation(&mut self, session_id: &str) {
        self.call(
            "subagent_new_session",
            json!({"agent": "explorer", "sessionId": session_id}),
        )
        .await;
    }

    /// One prompt turn, blocking for at most `grace` before it hands back a `responseId`.
    async fn prompt_within(&mut self, session_id: &str, text: &str, grace: Duration) -> ToolResult {
        self.call(
            "subagent_prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": text}],
                "graceMs": grace.as_millis() as u64
            }),
        )
        .await
    }

    async fn await_response(&mut self, response_id: &str, timeout: Duration) -> ToolResult {
        self.call(
            "subagent_await",
            json!({"responseId": response_id, "timeoutMs": timeout.as_millis() as u64}),
        )
        .await
    }

    async fn cancel_conversation(&mut self, session_id: &str) -> ToolResult {
        self.call("subagent_cancel", json!({"sessionId": session_id}))
            .await
    }

    async fn list_conversations(&mut self) -> ToolResult {
        self.call("subagent_list", json!({})).await
    }

    async fn advertised_tools(&mut self) -> Vec<Value> {
        let id = self.take_id();
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": {}}))
            .await;
        let response = self.read().await;
        response["result"]["tools"]
            .as_array()
            .unwrap_or_else(|| panic!("tools/list must return a tools array; got: {response}"))
            .clone()
    }

    async fn call(&mut self, name: &str, arguments: Value) -> ToolResult {
        let id = self.take_id();
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
        .await;
        let response = self.read().await;
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| {
                panic!("'{name}' must return a text content block; got: {response}")
            });
        let body = serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("'{name}' result {text:?} was not valid JSON: {e}"));
        ToolResult {
            tool: name.to_string(),
            body,
        }
    }

    fn take_id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        tokio::time::timeout(IO_TIMEOUT, self.stdin.write_all(line.as_bytes()))
            .await
            .expect("write to tddy-tools stdin timed out")
            .expect("write to tddy-tools stdin");
    }

    async fn read(&mut self) -> Value {
        let mut line = String::new();
        tokio::time::timeout(IO_TIMEOUT, self.stdout.read_line(&mut line))
            .await
            .expect("read from tddy-tools stdout timed out")
            .expect("read from tddy-tools stdout");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("invalid JSON-RPC line {line:?}: {e}"))
    }
}

impl Drop for SubagentTools {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

// ─── What a conversation tool answered ────────────────────────────────────────

struct ToolResult {
    tool: String,
    body: Value,
}

impl ToolResult {
    /// The turn finished and this is its answer.
    fn assert_yielded(&self, answer: &str) -> &Self {
        assert_eq!(
            self.body["stopReason"].as_str(),
            Some("end_turn"),
            "'{}' must report the turn ended; got: {}",
            self.tool,
            self.body
        );
        assert_eq!(
            self.body["content"][0]["text"].as_str(),
            Some(answer),
            "'{}' must carry the subagent's answer; got: {}",
            self.tool,
            self.body
        );
        self
    }

    /// The shape a caller written before response ids reads: an outcome, and nothing that would
    /// make it look like one.
    fn assert_carries_no_response_id(&self) -> &Self {
        assert!(
            self.body.get("responseId").is_none() && self.body.get("pending").is_none(),
            "'{}' answered inside its grace period, so it must return the outcome shape verbatim \
             — no 'responseId', no 'pending'; got: {}",
            self.tool,
            self.body
        );
        self
    }

    fn assert_usage(&self, total_tokens: u32) -> &Self {
        assert_eq!(
            self.body["usage"]["totalTokens"].as_u64(),
            Some(u64::from(total_tokens)),
            "'{}' must report the turn's token usage; got: {}",
            self.tool,
            self.body
        );
        self
    }

    /// The turn is still running, and this names it.
    fn assert_pending(&self) -> &Self {
        assert_eq!(
            self.body["pending"].as_bool(),
            Some(true),
            "'{}' must mark a turn that is still running as pending; got: {}",
            self.tool,
            self.body
        );
        assert!(
            self.body["responseId"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "'{}' must name the response the turn will land under; got: {}",
            self.tool,
            self.body
        );
        self
    }

    fn assert_failed_mentioning(&self, needle: &str) -> &Self {
        assert_eq!(
            self.body["is_error"].as_bool(),
            Some(true),
            "'{}' must report an error result; got: {}",
            self.tool,
            self.body
        );
        let error = self.body["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(needle),
            "'{}' error must mention {needle:?}; got: {error:?}",
            self.tool
        );
        self
    }

    fn response_id(&self) -> String {
        self.body["responseId"]
            .as_str()
            .unwrap_or_else(|| panic!("'{}' carried no responseId: {}", self.tool, self.body))
            .to_string()
    }

    /// The `subagent_list` row for `conversation_id`.
    fn conversation(&self, conversation_id: &str) -> &Value {
        self.body["conversations"]
            .as_array()
            .unwrap_or_else(|| {
                panic!(
                    "subagent_list must return conversations; got: {}",
                    self.body
                )
            })
            .iter()
            .find(|c| c["id"] == conversation_id)
            .unwrap_or_else(|| panic!("no conversation '{conversation_id}' in: {}", self.body))
    }
}

// ─── AC25: a fast turn is exactly what it is today ────────────────────────────

/// The whole point of a *grace* period rather than a timeout: a turn that yields inside it is
/// answered synchronously, in the shape every existing caller already parses.
#[tokio::test]
async fn a_turn_that_yields_inside_the_grace_period_returns_its_outcome_directly() {
    // Given a model that answers at once, and a conversation open with it
    let model = a_model_answering("src/auth.rs:1-50", Duration::ZERO).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-fast").await;

    // When the agent prompts it with a blocking budget the turn cannot exhaust
    let result = tools
        .prompt_within("conv-fast", "Where is the authentication logic?", AMPLE)
        .await;

    // Then the outcome comes back on the call itself
    result
        .assert_yielded("src/auth.rs:1-50")
        .assert_usage(PROMPT_TOKENS + COMPLETION_TOKENS)
        .assert_carries_no_response_id();
}

// ─── AC26/AC27: a slow turn becomes a response id ─────────────────────────────

/// A turn still running when the grace period elapses must hand the agent a receipt rather than
/// hold its only tool call open.
#[tokio::test]
async fn a_turn_still_running_at_the_grace_period_returns_a_response_id() {
    // Given a model slower than the caller's blocking budget
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-slow").await;

    // When the agent prompts it
    let result = tools
        .prompt_within(
            "conv-slow",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;

    // Then the call returns pending rather than the turn's answer or a timeout error
    result.assert_pending();
}

/// The receipt is redeemable: the turn was never cancelled, it kept running, and `subagent_await`
/// returns exactly what a fast `subagent_prompt` would have.
#[tokio::test]
async fn subagent_await_returns_the_background_turns_outcome() {
    // Given a turn deferred past its grace period
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-await").await;
    let deferred = tools
        .prompt_within(
            "conv-await",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;

    // When the agent awaits it with room for the turn to finish
    let collected = tools.await_response(&deferred.response_id(), AMPLE).await;

    // Then it collects the same outcome the prompt would have returned
    collected
        .assert_yielded("src/auth.rs:1-50")
        .assert_usage(PROMPT_TOKENS + COMPLETION_TOKENS);
}

/// An await is a bounded question, not a second way to hang: one that runs out reports the turn as
/// still pending, so the agent knows to ask again rather than that the answer is lost.
#[tokio::test]
async fn an_await_that_outruns_the_turn_reports_it_still_pending() {
    // Given a turn deferred past its grace period
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-impatient").await;
    let deferred = tools
        .prompt_within(
            "conv-impatient",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;

    // When the agent awaits it for less time than the turn still needs
    let collected = tools
        .await_response(&deferred.response_id(), Duration::from_millis(100))
        .await;

    // Then it is told the turn is still running, under the same response id
    collected.assert_pending();
    assert_eq!(
        collected.response_id(),
        deferred.response_id(),
        "an await that timed out must name the same response, or the agent cannot retry it"
    );
}

// ─── AC28: a failed turn is awaited as its failure ────────────────────────────

/// A turn that ends in an error has ended. Reporting it as pending forever would leave the agent
/// polling for an answer that will never come.
#[tokio::test]
async fn a_background_turn_that_fails_is_awaited_as_that_failure() {
    // Given a model that takes its time and then fails
    let model = a_model_failing(SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-doomed").await;
    let deferred = tools
        .prompt_within(
            "conv-doomed",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;

    // When the agent awaits it
    let collected = tools.await_response(&deferred.response_id(), AMPLE).await;

    // Then the failure is what it collects
    collected.assert_failed_mentioning("500");
}

// ─── AC29: collecting is repeatable, and an unknown id is an error ────────────

/// A tool result lost between this process and the main agent — a truncated frame, a compaction, a
/// restarted client — must not make a computed answer permanently unreachable.
#[tokio::test]
async fn the_same_response_id_can_be_awaited_more_than_once() {
    // Given a background turn already collected once
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-twice").await;
    let deferred = tools
        .prompt_within(
            "conv-twice",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;
    let response_id = deferred.response_id();
    tools.await_response(&response_id, AMPLE).await;

    // When the agent awaits the same response again
    let second = tools.await_response(&response_id, AMPLE).await;

    // Then it gets the same answer, not a missing one
    second.assert_yielded("src/auth.rs:1-50");
}

/// An id nothing was ever stored under is a caller mistake, and has to read as one — an await that
/// waited out its timeout on it would look exactly like a slow turn.
#[tokio::test]
async fn awaiting_an_unknown_response_id_is_an_error() {
    // Given a server with no turns in flight
    let model = a_model_answering("src/auth.rs:1-50", Duration::ZERO).await;
    let mut tools = SubagentTools::talking_to(&model).await;

    // When the agent awaits an id that was never handed out
    let collected = tools.await_response("response-that-never-was", AMPLE).await;

    // Then it is refused, naming the id
    collected.assert_failed_mentioning("response-that-never-was");
}

// ─── AC30: a second prompt queues behind the running turn ─────────────────────

/// A conversation's history is one sequence. A prompt arriving mid-turn therefore waits for the
/// running turn and then runs — proved by the second turn's model call carrying the first turn's
/// exchange, in order, rather than a history the two turns interleaved into.
#[tokio::test]
async fn a_prompt_arriving_mid_turn_queues_behind_the_running_one() {
    // Given a conversation with a slow turn already deferred
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-queue").await;
    let first = tools
        .prompt_within(
            "conv-queue",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;
    first.assert_pending();

    // When a second prompt arrives while that turn is still running
    let second = tools
        .prompt_within(
            "conv-queue",
            "Is there rate limiting there too?",
            SHORT_GRACE,
        )
        .await;
    second.assert_pending();
    tools.await_response(&first.response_id(), AMPLE).await;
    tools.await_response(&second.response_id(), AMPLE).await;

    // Then it ran after the first, against the history the first left behind
    let calls = model.received_requests().await.expect("model requests");
    assert_eq!(calls.len(), 2, "each prompt must run exactly one turn");
    assert_eq!(
        user_turns_of(&calls[0].body),
        vec!["Where is the authentication logic?"],
        "the first turn must run against a fresh conversation"
    );
    assert_eq!(
        user_turns_of(&calls[1].body),
        vec![
            "Where is the authentication logic?",
            "Is there rate limiting there too?"
        ],
        "the queued turn must run against the history the first turn left, in call order"
    );
}

// ─── AC31: cancelling resolves what is pending ────────────────────────────────

/// Closing a conversation with a turn in flight has to answer everyone waiting on that turn.
/// Leaving the response unresolved would park the agent on an await nobody will ever complete.
#[tokio::test]
async fn cancelling_a_conversation_resolves_its_pending_turn() {
    // Given a conversation with a turn deferred past its grace period
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-cancelled").await;
    let deferred = tools
        .prompt_within(
            "conv-cancelled",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;

    // When the agent closes the conversation out from under it
    tools.cancel_conversation("conv-cancelled").await;

    // Then awaiting the deferred turn is answered rather than left hanging
    tools
        .await_response(&deferred.response_id(), AMPLE)
        .await
        .assert_failed_mentioning("cancelled");
}

// ─── AC32: accounting follows the turn's end ──────────────────────────────────

/// A turn that has not ended has spent nothing the session can attribute to it yet — and asking
/// must not be blocked by the turn it is asking about.
///
/// The conversation deliberately has one *finished* turn behind it, so "the totals as of the last
/// completed turn" is a different number from "zero": a listing that reported the running turn, or
/// reported nothing at all, would both be visible here.
#[tokio::test]
async fn subagent_list_reports_pre_turn_totals_while_a_background_turn_runs() {
    // Given a conversation with one finished turn, and a second still running
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-inflight").await;
    tools
        .prompt_within("conv-inflight", "Where is the authentication logic?", AMPLE)
        .await
        .assert_yielded("src/auth.rs:1-50");
    tools
        .prompt_within(
            "conv-inflight",
            "Is there rate limiting there too?",
            SHORT_GRACE,
        )
        .await
        .assert_pending();

    // When the agent lists its conversations
    let listed = tools.list_conversations().await;

    // Then it is told what the conversation had spent before the running turn, not what it is
    // spending now
    let conversation = listed.conversation("conv-inflight");
    assert_eq!(
        conversation["turns"].as_u64(),
        Some(1),
        "only the finished turn may be counted; got: {conversation}"
    );
    assert_eq!(
        conversation["totalTokens"].as_u64(),
        Some(u64::from(PROMPT_TOKENS + COMPLETION_TOKENS)),
        "a turn still running must add nothing to the totals; got: {conversation}"
    );
}

/// Where the turn ran changes nothing about what it cost: once it ends, its usage is the session's
/// to account for, exactly as a synchronous turn's is.
#[tokio::test]
async fn subagent_list_reports_a_background_turns_usage_once_it_ends() {
    // Given a background turn that has been collected
    let model = a_model_answering("src/auth.rs:1-50", SLOW_TURN).await;
    let mut tools = SubagentTools::talking_to(&model).await;
    tools.open_conversation("conv-billed").await;
    let deferred = tools
        .prompt_within(
            "conv-billed",
            "Where is the authentication logic?",
            SHORT_GRACE,
        )
        .await;
    tools.await_response(&deferred.response_id(), AMPLE).await;

    // When the agent lists its conversations
    let listed = tools.list_conversations().await;

    // Then the finished turn is counted, with the tokens it spent
    let conversation = listed.conversation("conv-billed");
    assert_eq!(
        conversation["turns"].as_u64(),
        Some(1),
        "a finished background turn must be counted; got: {conversation}"
    );
    assert_eq!(
        conversation["totalTokens"].as_u64(),
        Some(u64::from(PROMPT_TOKENS + COMPLETION_TOKENS)),
        "a finished background turn must report what it spent; got: {conversation}"
    );
}

// ─── AC34: the schema is where the agent learns the second shape ──────────────

/// An agent handed a `responseId` needs a tool to redeem it with, and `tools/list` is the only
/// place it can discover one.
#[tokio::test]
async fn tools_list_advertises_subagent_await_while_an_agent_is_attached() {
    // Given a server with an agent attached
    let model = a_model_answering("src/auth.rs:1-50", Duration::ZERO).await;
    let mut tools = SubagentTools::talking_to(&model).await;

    // When the agent reads the catalog
    let advertised = tools.advertised_tools().await;

    // Then the tool that collects a deferred turn is in it
    let names: Vec<&str> = advertised
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(
        names.contains(&"subagent_await"),
        "tools/list must advertise 'subagent_await' alongside the conversation tools; got: {names:?}"
    );
}

/// A `responseId` where an answer was expected reads as a malformed result unless the schema says
/// otherwise. The description is the only thing the main agent has to go on.
#[tokio::test]
async fn the_advertised_prompt_tool_documents_both_return_shapes() {
    // Given a server with an agent attached
    let model = a_model_answering("src/auth.rs:1-50", Duration::ZERO).await;
    let mut tools = SubagentTools::talking_to(&model).await;

    // When the agent reads what subagent_prompt returns
    let advertised = tools.advertised_tools().await;
    let prompt_tool = advertised
        .iter()
        .find(|t| t["name"] == "subagent_prompt")
        .expect("subagent_prompt must be advertised while an agent is attached");
    let description = prompt_tool["description"].as_str().unwrap_or_default();

    // Then both shapes are named, and so is the tool that collects the deferred one
    for phrase in ["stopReason", "responseId", "subagent_await"] {
        assert!(
            description.contains(phrase),
            "the subagent_prompt description must mention {phrase:?} so the agent can read a \
             deferred turn as deferred; got: {description:?}"
        );
    }
}
