//! Unit tests: `SubagentRegistry::from_defs` — a subagent session built from a YAML-defined
//! `SpecializedAgentDef` rather than the single hardcoded `"fastcontext"` factory.
//!
//! Feature: docs/ft/coder/specialized-subagents.md (criteria 5-8)
//! Changeset: docs/dev/1-WIP/specialized-subagents.md
//!
//! `subagent_session_red.rs` covers the existing `SubagentRegistry::new()` / `"fastcontext"`
//! factory path (unchanged by this generalization); this file covers the new `from_defs` path.

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{CodebaseAccess, StopReason, SubagentConfig, SubagentRegistry};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn final_answer_response(answer: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": format!("Looked at the code.\n<final_answer>\n{answer}\n</final_answer>")
            },
            "finish_reason": "stop"
        }]
    })
}

/// A turn with plain assistant text — no tool call, no `<final_answer>`.
fn plain_text_response(text: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }]
    })
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
                    "function": {
                        "name": tool_name,
                        "arguments": args.to_string()
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn a_def(name: &str, base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: name.to_string(),
        label: None,
        model: "some-model".to_string(),
        base_url: base_url.to_string(),
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns: 6,
        replaces: vec![],
    }
}

fn empty_access_config() -> SubagentConfig {
    SubagentConfig {
        base_url: String::new(),
        model: String::new(),
        max_turns: 0,
        access: CodebaseAccess::Local,
    }
}

/// `SubagentRegistry::from_defs` resolves a session for each registered def by name, sending
/// requests to *that* def's `base_url`/`model` — proving multiple specialized agents can be
/// registered and addressed independently, not just the single hardcoded fastcontext factory.
#[tokio::test]
async fn registry_from_defs_creates_a_session_using_the_matching_defs_model_and_base_url() {
    // Given — two defs pointing at two different mock servers
    let server_a = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("a.rs:1-1")))
        .mount(&server_a)
        .await;
    let server_b = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("b.rs:1-1")))
        .mount(&server_b)
        .await;
    let registry = SubagentRegistry::from_defs(vec![
        a_def("agent-a", &server_a.uri()),
        a_def("agent-b", &server_b.uri()),
    ]);

    // When
    let mut session_b = registry
        .create("agent-b", empty_access_config())
        .expect("registry must resolve a def registered via from_defs");
    let outcome = session_b
        .prompt("Where is the entry point?")
        .await
        .expect("prompt must succeed");

    // Then — the request went to agent-b's server, not agent-a's
    assert_eq!(outcome.content[0].text, "b.rs:1-1");
    assert_eq!(
        server_a.received_requests().await.unwrap().len(),
        0,
        "agent-a's server must not receive a request when the caller asked for agent-b"
    );
}

/// A def's `system_prompt` (when set) seeds the conversation's first message — today's
/// `FastContextSession` (the `new()` / `"fastcontext"` factory path) starts with no system message
/// at all.
#[tokio::test]
async fn a_defs_system_prompt_seeds_the_first_message_of_the_conversation() {
    // Given
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("x.rs:1-1")))
        .mount(&server)
        .await;
    let mut def = a_def("prompted-agent", &server.uri());
    def.system_prompt = Some("You are a terse codebase explorer.".to_string());
    let registry = SubagentRegistry::from_defs(vec![def]);
    let mut session = registry
        .create("prompted-agent", empty_access_config())
        .expect("registry must resolve the registered def");

    // When
    session
        .prompt("Where is the entry point?")
        .await
        .expect("prompt must succeed");

    // Then — the request's first message has role "system" and the def's prompt text
    let calls = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&calls[0].body).unwrap();
    let messages = body["messages"].as_array().expect("messages array");
    assert_eq!(
        messages[0]["role"], "system",
        "the first message must be a system message when the def sets system_prompt"
    );
    assert_eq!(
        messages[0]["content"], "You are a terse codebase explorer.",
        "the system message must carry the def's system_prompt text verbatim"
    );
}

/// A def binding only `[READ]` does not advertise `GLOB`/`GREP` tool schemas to the model — the
/// model only sees the tools the def actually bound.
#[tokio::test]
async fn a_def_binding_only_read_does_not_advertise_glob_or_grep_to_the_model() {
    // Given — a def bound to READ only
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("x.rs:1-1")))
        .mount(&server)
        .await;
    let mut def = a_def("read-only-agent", &server.uri());
    def.tools = vec![SubagentTool::Read];
    let registry = SubagentRegistry::from_defs(vec![def]);
    let mut session = registry
        .create("read-only-agent", empty_access_config())
        .expect("registry must resolve the registered def");

    // When
    session
        .prompt("Where is the entry point?")
        .await
        .expect("prompt must succeed");

    // Then — the request's tools array names only READ
    let calls = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&calls[0].body).unwrap();
    let tool_names: Vec<&str> = body["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["function"]["name"].as_str())
        .collect();
    assert_eq!(
        tool_names,
        vec!["READ"],
        "only the def's bound tools may be advertised to the model; got: {tool_names:?}"
    );
}

/// A model-issued call to a tool the def did not bind must be rejected (a typed error surfaced as
/// a `tool`-role error message), not silently executed as if it had been bound.
#[tokio::test]
async fn a_model_issued_call_to_an_unbound_tool_is_rejected_not_silently_ignored() {
    // Given — a READ-only def, but the mock model calls GREP anyway (simulating a
    // misbehaving/older model that ignores the advertised tool list)
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tool_call_response(
            "GREP",
            serde_json::json!({"pattern": "fn main"}),
        )))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(final_answer_response("x.rs:1-1")))
        .mount(&server)
        .await;
    let mut def = a_def("read-only-agent", &server.uri());
    def.tools = vec![SubagentTool::Read];
    let registry = SubagentRegistry::from_defs(vec![def]);
    let mut session = registry
        .create("read-only-agent", empty_access_config())
        .expect("registry must resolve the registered def");

    // When
    session
        .prompt("Search for fn main")
        .await
        .expect("prompt must still resolve (the loop reports the rejection, not panics)");

    // Then — the second request's tool-result message for the GREP call reports an error, not a
    // successful grep result
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        calls.len(),
        2,
        "the loop must continue after reporting the rejection"
    );
    let second_body: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
    let tool_message = second_body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("a tool-result message must be present for the rejected GREP call");
    let content = tool_message["content"].as_str().unwrap_or_default();
    assert!(
        content.to_lowercase().contains("error") || content.to_lowercase().contains("not bound"),
        "the tool-result for an unbound tool call must report an error, not a real grep result; got: {content:?}"
    );
}

/// A prompt turn that yields no tool call and no `<final_answer>` (plain prose, e.g. from an
/// agent without FastContext's citation convention) terminates `StopReason::EndTurn` with the
/// assistant's text as content — today only `<final_answer>` terminates `EndTurn`; without this, a
/// plain-prose agent loops until `max_turns` on every single-turn answer.
#[tokio::test]
async fn a_prose_only_turn_with_no_tool_call_and_no_final_answer_terminates_end_turn() {
    // Given — the model answers in plain prose, no tool call, no <final_answer> block
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(plain_text_response(
                "The entry point is main() in src/main.rs.",
            )),
        )
        .mount(&server)
        .await;
    let def = a_def("prose-agent", &server.uri());
    let registry = SubagentRegistry::from_defs(vec![def]);
    let mut session = registry
        .create("prose-agent", empty_access_config())
        .expect("registry must resolve the registered def");

    // When
    let outcome = session
        .prompt("Where is the entry point?")
        .await
        .expect("prompt must succeed");

    // Then — the loop terminated immediately with EndTurn, not MaxTurnRequests
    assert_eq!(
        outcome.stop_reason,
        StopReason::EndTurn,
        "a plain-prose, no-tool-call turn must terminate EndTurn, not loop toward MaxTurnRequests"
    );
    assert_eq!(
        outcome.content[0].text,
        "The entry point is main() in src/main.rs."
    );
    let calls = server.received_requests().await.unwrap();
    assert_eq!(
        calls.len(),
        1,
        "exactly one model call must be made — the loop must not keep going after plain prose"
    );
}

/// API boundary: a registry built via `from_defs` does **not** inherit the legacy hardcoded
/// `"fastcontext"` factory from `SubagentRegistry::new()` — the two construction paths are
/// independent. Requesting a name that is in neither the (empty) legacy factories map nor the
/// (empty) `defs` list is a normal "unknown subagent" error, not a panic — this exercises the
/// registry's fallback branch without touching the still-unimplemented def-resolution path.
#[test]
fn from_defs_does_not_fall_back_to_the_legacy_hardcoded_fastcontext_factory() {
    // Given — a registry built via from_defs with no matching "fastcontext" def registered
    let registry = SubagentRegistry::from_defs(vec![]);
    let config = empty_access_config();

    // When
    let result = registry.create("fastcontext", config);

    // Then — a normal typed error, not a panic and not a session built from the legacy factory
    assert!(
        result.is_err(),
        "from_defs must not silently fall back to the legacy 'fastcontext' factory when no \
         matching def is registered"
    );
    let message = result.err().unwrap().to_string();
    assert!(
        message.contains("fastcontext"),
        "error message must name the unresolved subagent; got: {message:?}"
    );
}
