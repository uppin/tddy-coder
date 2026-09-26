//! Integration tests: a conversation can be continued, and it can be sent back in time.
//!
//! Two wants, from the same incident. A budget that runs out mid-chain should be extendable
//! rather than only restartable — re-asking makes the agent re-read everything it already read.
//! And an agent that went the wrong way should be correctable at the step where it went wrong,
//! rather than abandoned.
//!
//! ## Why a rewind carries a correction
//!
//! Requests go out at `temperature: 0.0` (`subagent.rs`, `ChatCompletionRequest`). Rewinding and
//! re-running with identical context therefore reproduces the same turn, deterministically. The
//! incident demonstrated exactly that: prompted twice against the same history, FastContext
//! returned the same 1,470-character fabrication byte for byte. A rewind that appended nothing
//! would be a feature that silently does nothing, so `correction` is what makes the rewind mean
//! something.
//!
//! The assertions here are on **the messages the provider received**, because that is the only
//! place a rewind is observable. An outcome cannot show you what was left out.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md § Turn control

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, MessageId, MessageRole, PromptOutcome, SubagentConfig, SubagentRegistry,
    SubagentSession, TurnRequest,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

const THE_GOAL: &str = "Find where the scrollback size is set";
const A_CORRECTION: &str = "GhosttyTerminal.tsx is under components/, not lib/";
const THE_JAIL_IS_DEAD: &str =
    "session 01a0dd54: the tool call could not be run in its jail (its channel is closed)";
const A_BUDGET: u32 = 3;

fn a_def(base_url: &str, max_turns: u32) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns,
        replaces: Vec::new(),
    }
}

fn a_codebase_that_answers() -> CodebaseAccess {
    CodebaseAccess::managed(
        |_tool: String,
         _args: serde_json::Value|
         -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            Box::pin(async move {
                serde_json::json!({ "content": "nothing here", "truncated": false, "total_lines": 1 })
                    .to_string()
            })
        },
    )
}

fn a_codebase_whose_jail_is_dead() -> CodebaseAccess {
    CodebaseAccess::managed(
        |_tool: String,
         _args: serde_json::Value|
         -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            Box::pin(async move {
                serde_json::json!({ "is_error": true, "error": THE_JAIL_IS_DEAD }).to_string()
            })
        },
    )
}

fn a_turn_that_reads(path: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Looking there.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "READ",
                        "arguments": serde_json::json!({ "path": path }).to_string()
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

fn a_final_answer(text: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }]
    })
}

/// Reads once, then answers, then answers again — enough shape for a rewind to have somewhere to
/// go back to.
async fn a_model_that_reads_then_answers() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(a_turn_that_reads("src/lib/terminal.ts")),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer("could not find it")))
        .mount(&server)
        .await;
    server
}

/// Reaches for a tool on every turn of the budget, then answers. A prompt against a dead jail
/// therefore ends the way `M4` defines a tool outage — the budget spent with nothing read — rather
/// than ending early on a model answer. The turn *after* the budget is a plain prose answer, so a
/// resume against the same dead jail still succeeds: no tool call means no outage.
async fn a_model_that_exhausts_its_budget_then_answers() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(a_turn_that_reads("src/lib/terminal.ts")),
        )
        .up_to_n_times(u64::from(A_BUDGET))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer("could not find it")))
        .mount(&server)
        .await;
    server
}

fn a_session_over(
    server: &MockServer,
    access: CodebaseAccess,
    max_turns: u32,
) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri(), max_turns)])
        .create("explorer", SubagentConfig { access })
        .expect("the def must resolve")
}

/// Every `messages` array the provider was sent, in order, as `(role, content)` pairs.
async fn histories_the_model_received(server: &MockServer) -> Vec<Vec<(String, String)>> {
    server
        .received_requests()
        .await
        .expect("the mock provider records its requests")
        .iter()
        .map(|request| {
            let body: serde_json::Value =
                serde_json::from_slice(&request.body).expect("a chat request body is JSON");
            body["messages"]
                .as_array()
                .expect("a chat request carries messages")
                .iter()
                .map(|m| {
                    (
                        m["role"].as_str().unwrap_or_default().to_string(),
                        m["content"].as_str().unwrap_or_default().to_string(),
                    )
                })
                .collect()
        })
        .collect()
}

async fn the_last_history_the_model_received(server: &MockServer) -> Vec<(String, String)> {
    histories_the_model_received(server)
        .await
        .pop()
        .expect("the model was sent at least one turn")
}

fn the_id_of_the_first(outcome: &PromptOutcome, role: MessageRole) -> MessageId {
    outcome
        .messages
        .iter()
        .find(|m| m.role == role)
        .unwrap_or_else(|| panic!("the turn appended no {role:?} message"))
        .id
        .clone()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// The plain case: the agent was mid-chain when its budget ran out, and the caller wants it to
/// carry on. No new question is asked.
#[tokio::test]
async fn resuming_continues_the_conversation_without_sending_a_new_prompt_turn() {
    // Given a conversation that has already taken a turn
    let server = a_model_that_reads_then_answers().await;
    let mut session = a_session_over(&server, a_codebase_that_answers(), A_BUDGET);
    let first = session.prompt(THE_GOAL).await.expect("an answer");
    let history_before = the_last_history_the_model_received(&server).await;

    // When it is resumed with no new input
    let resumed = session
        .take_turn(TurnRequest::resuming())
        .await
        .expect("a resume continues a conversation");

    // Then the model was sent the history it already had, with nothing appended by the caller
    let history_after = the_last_history_the_model_received(&server).await;
    assert!(
        history_after.len() >= history_before.len(),
        "a resume continues rather than truncates"
    );
    assert_eq!(
        history_after
            .iter()
            .filter(|(role, content)| role == "user" && content == THE_GOAL)
            .count(),
        1,
        "the original prompt must not be asked a second time"
    );
    assert!(
        first
            .messages
            .iter()
            .all(|m| !resumed.messages.iter().any(|later| later.id == m.id)),
        "a resumed turn reports its own new messages, not the previous turn's"
    );
}

/// Going back in time. Everything after the named message is gone from what the model is sent.
#[tokio::test]
async fn resuming_from_an_earlier_message_discards_everything_after_it() {
    // Given a conversation whose first turn read the wrong file and concluded wrongly
    let server = a_model_that_reads_then_answers().await;
    let mut session = a_session_over(&server, a_codebase_that_answers(), A_BUDGET);
    let first = session.prompt(THE_GOAL).await.expect("an answer");
    let the_prompt = the_id_of_the_first(&first, MessageRole::User);

    // When the caller rewinds to just after the prompt
    session
        .take_turn(TurnRequest::resuming().from_message(the_prompt))
        .await
        .expect("a rewind is not a failure");

    // Then the model is sent the prompt and nothing that followed it
    let history = the_last_history_the_model_received(&server).await;
    assert_eq!(
        history,
        vec![("user".to_string(), THE_GOAL.to_string())],
        "everything after the rewind point must be gone, not merely marked"
    );
}

/// The correction is what makes a rewind able to change anything at all, given `temperature: 0.0`.
#[tokio::test]
async fn resuming_with_a_correction_appends_exactly_one_instruction_after_the_rewind_point() {
    // Given the same wrongly-concluded conversation
    let server = a_model_that_reads_then_answers().await;
    let mut session = a_session_over(&server, a_codebase_that_answers(), A_BUDGET);
    let first = session.prompt(THE_GOAL).await.expect("an answer");
    let the_prompt = the_id_of_the_first(&first, MessageRole::User);

    // When the caller rewinds and says what was wrong
    session
        .take_turn(
            TurnRequest::resuming()
                .from_message(the_prompt)
                .with_correction(A_CORRECTION),
        )
        .await
        .expect("a correction is not a failure");

    // Then the model sees the prompt followed by exactly one corrective instruction
    let history = the_last_history_the_model_received(&server).await;
    assert_eq!(
        history,
        vec![
            ("user".to_string(), THE_GOAL.to_string()),
            ("user".to_string(), A_CORRECTION.to_string()),
        ],
        "a correction is one message in one place; anything else is a second conversation"
    );
}

/// A rewind point the caller invented, or one a previous rewind already discarded, is a mistake
/// worth reporting. Continuing from the end instead would silently do the opposite of what was
/// asked.
#[tokio::test]
async fn resuming_from_an_unknown_message_is_refused_rather_than_continued() {
    // Given a conversation
    let server = a_model_that_reads_then_answers().await;
    let mut session = a_session_over(&server, a_codebase_that_answers(), A_BUDGET);
    session.prompt(THE_GOAL).await.expect("an answer");
    let turns_so_far = histories_the_model_received(&server).await.len();

    // When a resume names a message that does not exist
    let result = session
        .take_turn(TurnRequest::resuming().from_message(MessageId::from("m-never-minted")))
        .await;

    // Then it is refused, and no turn was spent guessing
    let error = result
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| panic!("an unknown rewind point must not silently continue"));
    assert!(
        error.contains("m-never-minted"),
        "the refusal must name the id the caller got wrong, got: {error}"
    );
    assert_eq!(
        histories_the_model_received(&server).await.len(),
        turns_so_far,
        "a refused resume must not reach the model"
    );
}

/// OpenAI rejects an assistant message carrying `tool_calls` that no `tool` message answers. A
/// rewind landing between the two would produce a history the provider refuses — a self-inflicted
/// error, at the exact moment the caller is trying to recover from one.
#[tokio::test]
async fn a_rewind_never_separates_a_tool_call_from_its_result() {
    // Given a conversation whose turn called a tool
    let server = a_model_that_reads_then_answers().await;
    let mut session = a_session_over(&server, a_codebase_that_answers(), A_BUDGET);
    let first = session.prompt(THE_GOAL).await.expect("an answer");
    let the_call = the_id_of_the_first(&first, MessageRole::Assistant);

    // When the caller rewinds to the assistant message that made the call — the message whose
    // result would be orphaned by a naive truncation
    session
        .take_turn(TurnRequest::resuming().from_message(the_call))
        .await
        .expect("a rewind onto a tool call is a legal request");

    // Then what the model receives is a history it can accept
    let history = the_last_history_the_model_received(&server).await;
    let assistant_calls = history
        .iter()
        .filter(|(role, _)| role == "assistant")
        .count();
    let tool_results = history.iter().filter(|(role, _)| role == "tool").count();
    assert_eq!(
        assistant_calls, tool_results,
        "every assistant tool call in the rewound history must still have its result: \
         {history:?}"
    );
}

/// The loose end from making a total tool outage an error: an error must still leave something to
/// go back to. The history belongs to the conversation, not to the outcome.
#[tokio::test]
async fn a_conversation_whose_prompt_failed_can_still_be_rewound_into() {
    // Given a prompt that spent its whole budget on tool calls that were all refused
    let server = a_model_that_exhausts_its_budget_then_answers().await;
    let mut session = a_session_over(&server, a_codebase_whose_jail_is_dead(), A_BUDGET);
    let failed = session.prompt(THE_GOAL).await;
    assert!(failed.is_err(), "a total tool outage is an error");

    // When the caller resumes it with a correction
    session
        .take_turn(TurnRequest::resuming().with_correction("the jail is back; try again"))
        .await
        .expect("a failed prompt leaves a conversation, not a wreck");

    // Then the failed prompt's own exchanges are still there to build on
    let history = the_last_history_the_model_received(&server).await;
    assert!(
        history
            .iter()
            .any(|(role, content)| role == "user" && content == THE_GOAL),
        "the failed prompt must survive in the history it grew: {history:?}"
    );
}
