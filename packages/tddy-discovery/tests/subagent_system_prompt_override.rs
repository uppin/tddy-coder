//! Integration tests: the agent opening a conversation may supply the system prompt for it,
//! instead of being stuck with whatever the def was written with.
//!
//! A def's `system_prompt` is authored once, for every use of that agent. A main agent delegating
//! a specific piece of work knows things the def's author could not: which repository this is,
//! what shape the answer has to take, and — after a turn that went wrong — what the agent must
//! stop doing. In session 01a0e200 the only correction available was to send another prompt into
//! the same conversation, which is a message the model weighs against everything already in its
//! window; a system prompt is not.
//!
//! The rule these tests pin: an override replaces the def's prompt for that conversation and
//! nothing else, and a blank one is refused rather than quietly seeding an empty system message.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{CodebaseAccess, SubagentConfig, SubagentRegistry};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

const THE_DEFS_OWN_PROMPT: &str = "You are a fast codebase explorer. Answer in one paragraph.";
const AN_OVERRIDE: &str =
    "You are searching a React terminal component. Name files, never quote them.";
const THE_GOAL: &str = "Where does the terminal switch to history on scroll?";

// ─── Builders ────────────────────────────────────────────────────────────────

fn a_def_with_prompt(base_url: &str, system_prompt: Option<&str>) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: system_prompt.map(str::to_string),
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns: 2,
        replaces: Vec::new(),
    }
}

fn a_codebase_that_answers() -> CodebaseAccess {
    CodebaseAccess::managed(
        |_tool: String,
         _args: serde_json::Value|
         -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            Box::pin(async move {
                serde_json::json!({ "content": "ok", "truncated": false, "total_lines": 1 })
                    .to_string()
            })
        },
    )
}

/// A provider that answers every turn immediately, so each test spends exactly one request and
/// the assertion is about what was *sent*.
async fn a_model_that_answers() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "terminalPresentation.ts" },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;
    server
}

/// The system messages the provider actually received on its first request, in order.
async fn system_messages_sent_to(server: &MockServer) -> Vec<String> {
    let requests = server
        .received_requests()
        .await
        .expect("the mock provider records its requests");
    let first = requests.first().expect("the turn sends one request");
    let body: serde_json::Value =
        serde_json::from_slice(&first.body).expect("the request body is JSON");
    body["messages"]
        .as_array()
        .expect("a chat request carries messages")
        .iter()
        .filter(|m| m["role"] == "system")
        .map(|m| m["content"].as_str().unwrap_or_default().to_string())
        .collect()
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The control: with no override, nothing about today's behaviour changes.
#[tokio::test]
async fn a_conversation_started_without_an_override_sends_the_defs_own_system_prompt() {
    // Given a def that declares a system prompt, opened without an override
    let server = a_model_that_answers().await;
    let mut session = SubagentRegistry::from_defs(vec![a_def_with_prompt(
        &server.uri(),
        Some(THE_DEFS_OWN_PROMPT),
    )])
    .create("explorer", SubagentConfig::new(a_codebase_that_answers()))
    .expect("the def must resolve");

    // When it runs a turn
    session.prompt(THE_GOAL).await.expect("an answer");

    // Then the def's prompt is what the model saw
    assert_eq!(
        system_messages_sent_to(&server).await,
        vec![THE_DEFS_OWN_PROMPT.to_string()]
    );
}

/// The feature: the caller's prompt is the one the model gets, and the def's is gone — not
/// appended to, which would leave the model weighing two sets of instructions.
#[tokio::test]
async fn an_override_replaces_the_defs_system_prompt_for_that_conversation() {
    // Given a def that declares a system prompt, opened with an override
    let server = a_model_that_answers().await;
    let mut session = SubagentRegistry::from_defs(vec![a_def_with_prompt(
        &server.uri(),
        Some(THE_DEFS_OWN_PROMPT),
    )])
    .create(
        "explorer",
        SubagentConfig::new(a_codebase_that_answers()).with_system_prompt(AN_OVERRIDE),
    )
    .expect("the def must resolve");

    // When it runs a turn
    session.prompt(THE_GOAL).await.expect("an answer");

    // Then only the override reached the model
    assert_eq!(
        system_messages_sent_to(&server).await,
        vec![AN_OVERRIDE.to_string()]
    );
}

/// An agent whose def declares no prompt can still be given one for a single piece of work.
#[tokio::test]
async fn an_override_seeds_a_system_prompt_for_a_def_that_declares_none() {
    // Given a def with no system prompt, opened with an override
    let server = a_model_that_answers().await;
    let mut session = SubagentRegistry::from_defs(vec![a_def_with_prompt(&server.uri(), None)])
        .create(
            "explorer",
            SubagentConfig::new(a_codebase_that_answers()).with_system_prompt(AN_OVERRIDE),
        )
        .expect("the def must resolve");

    // When it runs a turn
    session.prompt(THE_GOAL).await.expect("an answer");

    // Then the override is the conversation's system prompt
    assert_eq!(
        system_messages_sent_to(&server).await,
        vec![AN_OVERRIDE.to_string()]
    );
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// The def is not edited by using it. A second conversation with the same agent gets the def's
/// prompt back, so an override cannot leak into work the caller did not scope it to.
#[tokio::test]
async fn an_override_is_scoped_to_its_own_conversation_and_leaves_the_def_untouched() {
    // Given one registry, and a first conversation opened with an override
    let server = a_model_that_answers().await;
    let registry = SubagentRegistry::from_defs(vec![a_def_with_prompt(
        &server.uri(),
        Some(THE_DEFS_OWN_PROMPT),
    )]);
    let mut overridden = registry
        .create(
            "explorer",
            SubagentConfig::new(a_codebase_that_answers()).with_system_prompt(AN_OVERRIDE),
        )
        .expect("the def must resolve");
    overridden.prompt(THE_GOAL).await.expect("an answer");

    // When a second conversation is opened from the same registry without one
    let mut plain = registry
        .create("explorer", SubagentConfig::new(a_codebase_that_answers()))
        .expect("the def must resolve");
    plain.prompt(THE_GOAL).await.expect("an answer");

    // Then the second conversation saw the def's own prompt
    let requests = server
        .received_requests()
        .await
        .expect("the mock provider records its requests");
    let second: serde_json::Value =
        serde_json::from_slice(&requests[1].body).expect("the second request body is JSON");
    assert_eq!(second["messages"][0]["content"], THE_DEFS_OWN_PROMPT);
}

/// A blank override is a caller that meant to send something. Seeding an empty system message
/// would spend a turn's prefix on nothing and read, in the transcript, as a deliberate silence.
#[test]
fn a_blank_override_is_refused_rather_than_seeding_an_empty_system_message() {
    // Given a def and an override that is only whitespace
    let registry = SubagentRegistry::from_defs(vec![a_def_with_prompt(
        "http://127.0.0.1:1",
        Some(THE_DEFS_OWN_PROMPT),
    )]);

    // When a conversation is opened with it
    let opened = registry.create(
        "explorer",
        SubagentConfig::new(a_codebase_that_answers()).with_system_prompt("   "),
    );

    // Then it is refused, naming the field
    let error = opened
        .err()
        .expect("a blank system prompt must not open a conversation");
    assert!(
        error.to_string().contains("system prompt"),
        "the refusal must name the field the caller got wrong; got: {error}"
    );
}
