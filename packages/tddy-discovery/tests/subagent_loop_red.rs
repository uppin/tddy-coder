//! Integration tests: subagent turn-loop behaviours that address the fastcontext runaway in
//! session 019f2d14.
//!
//!   A. READ-window wiring — a model-issued `READ` with `offset`/`limit` must come back windowed,
//!      proving `dispatch_tool_call` actually reads those args (today they are silently dropped).
//!
//!   B. Forced synthesis on budget exhaustion — when the per-prompt turn budget runs out with no
//!      `<final_answer>`, the session must spend one final tool-less turn to synthesise findings
//!      from what it gathered, instead of returning empty content. In 019f2d14 the model burned all
//!      10 turns tool-calling (every turn `content=0 chars`) and the caller got nothing back.
//!
//! Root-cause: packages/tddy-discovery/src/subagent.rs (`dispatch_tool_call`, `FastContextSession`).

use tddy_discovery::subagent::{CodebaseAccess, StopReason, SubagentConfig, SubagentRegistry};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── Mock model responses ───────────────────────────────────────────────────────

fn final_answer_response(answer: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }]
    })
}

/// A plain-prose assistant turn with no tool calls and no `<final_answer>` tag — what the model
/// emits when explicitly told to stop searching and summarise.
fn prose_response(text: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }]
    })
}

fn glob_tool_call_response() -> serde_json::Value {
    tool_call_response("GLOB", serde_json::json!({ "pattern": "**/*.rs" }))
}

fn read_window_tool_call_response(path: &str, offset: u64, limit: u64) -> serde_json::Value {
    tool_call_response(
        "READ",
        serde_json::json!({ "path": path, "offset": offset, "limit": limit }),
    )
}

fn tool_call_response(tool_name: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": { "name": tool_name, "arguments": args.to_string() }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn a_local_config(base_url: &str, max_turns: u32) -> SubagentConfig {
    SubagentConfig {
        base_url: base_url.to_string(),
        model: "microsoft/FastContext-1.0-4B-RL".to_string(),
        max_turns,
        access: CodebaseAccess::Local,
    }
}

fn numbered_lines(count: usize) -> String {
    (0..count)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── A. READ-window wiring ────────────────────────────────────────────────────

/// A model-issued `READ` carrying `offset`/`limit` must reach the codebase windowed: the
/// tool-result fed back to the model is the requested slice, not the entire file.
#[tokio::test]
async fn a_read_tool_call_with_offset_and_limit_returns_a_windowed_tool_result() {
    // Given — a 500-line file, and a model that issues one windowed READ then finalises
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("big.rs");
    std::fs::write(&file, numbered_lines(500)).expect("write file");
    let file_path = file.to_str().unwrap().to_string();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(read_window_tool_call_response(&file_path, 300, 10)),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("done")))
        .mount(&server)
        .await;
    let mut session = SubagentRegistry::new()
        .create("fastcontext", a_local_config(&server.uri(), 6))
        .expect("fastcontext must be registered");

    // When
    session
        .prompt("Read the middle of big.rs")
        .await
        .expect("prompt must succeed");

    // Then — the tool-result carried into the second request is the 10-line window (300..310)
    let calls = server.received_requests().await.unwrap();
    let second: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
    let tool_message = second["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("a tool-result message for the READ call must be present");
    let payload: serde_json::Value =
        serde_json::from_str(tool_message["content"].as_str().unwrap())
            .expect("tool-result content must be a JSON READ payload");
    let content = payload["content"]
        .as_str()
        .expect("windowed READ payload must carry 'content'");
    assert_eq!(
        content.lines().count(),
        10,
        "READ with limit=10 must return exactly 10 lines, not the whole file; got {} lines",
        content.lines().count()
    );
    assert_eq!(
        content.lines().next(),
        Some("line 300"),
        "READ with offset=300 must start at line 300"
    );
}

// ─── B. Forced synthesis on budget exhaustion ─────────────────────────────────

/// When the turn budget is exhausted with no `<final_answer>`, the session spends one final
/// tool-less turn to synthesise findings and returns that prose — never empty content.
#[tokio::test]
async fn exhausting_the_turn_budget_forces_one_synthesis_turn_and_returns_its_findings() {
    // Given — the model tool-calls on every search turn (never finalises); the synthesis turn
    // afterwards returns prose
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(glob_tool_call_response()))
        .up_to_n_times(3)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(prose_response(
            "Config is loaded in src/config.rs:12; the daemon reads it at src/daemon.rs:88.",
        )))
        .mount(&server)
        .await;
    let mut session = SubagentRegistry::new()
        .create("fastcontext", a_local_config(&server.uri(), 3))
        .expect("fastcontext must be registered");

    // When
    let outcome = session
        .prompt("Find where config is loaded")
        .await
        .expect("prompt must return Ok even when the turn budget is exhausted");

    // Then — three search turns plus one synthesis turn, and the synthesis prose is returned
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        calls.len(),
        4,
        "three search turns plus one forced synthesis turn must be made; got {}",
        calls.len()
    );
    assert_eq!(outcome.stop_reason, StopReason::MaxTurnRequests);
    assert_eq!(
        outcome.content.len(),
        1,
        "the synthesis turn must yield exactly one content block, not empty content"
    );
    assert_eq!(
        outcome.content[0].text,
        "Config is loaded in src/config.rs:12; the daemon reads it at src/daemon.rs:88."
    );
}

/// The forced synthesis turn advertises no tools, so the model cannot keep searching and is
/// compelled to answer from the context it already gathered.
#[tokio::test]
async fn the_forced_synthesis_turn_advertises_no_tools() {
    // Given — same exhaustion setup: three tool-calling turns, then a prose synthesis turn
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(glob_tool_call_response()))
        .up_to_n_times(3)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(prose_response("Answered.")))
        .mount(&server)
        .await;
    let mut session = SubagentRegistry::new()
        .create("fastcontext", a_local_config(&server.uri(), 3))
        .expect("fastcontext must be registered");

    // When
    session
        .prompt("Find where config is loaded")
        .await
        .expect("prompt must succeed");

    // Then — the fourth (synthesis) request carries an empty tools array
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        calls.len(),
        4,
        "a forced synthesis turn must follow the three exhausted search turns; got {}",
        calls.len()
    );
    let synthesis_body: serde_json::Value = serde_json::from_slice(&calls[3].body).unwrap();
    let tools = synthesis_body["tools"]
        .as_array()
        .expect("request body must carry a tools array");
    assert!(
        tools.is_empty(),
        "the synthesis turn must advertise no tools so the model cannot keep searching; got: {tools:?}"
    );
}
