//! Integration tests: what a specialized agent does when it runs out of **context**, as opposed to
//! running out of turns.
//!
//! Turn exhaustion already lands softly — `run_synthesis_turn` spends one tool-less turn so the
//! caller gets what was gathered, under `StopReason::MaxTurnRequests`. Context exhaustion had no
//! such landing: the provider's refusal propagated verbatim, the turn's work was lost, and because
//! `messages` is conversation-scoped and never pruned, every later prompt re-sent the same
//! oversized history and failed the same way. The conversation was wedged until it was cancelled.
//!
//! The landing here cannot be model-generated — the context that would summarise the work is the
//! context that is full — so the brief is built mechanically from the history on one rule: drop the
//! payloads, keep the conclusions and the index of what was already examined. That index is the
//! part that stops a replacement conversation re-reading the same files.
//!
//! Changeset: docs/dev/1-WIP/2026-09-20-specialized-agent-context-handoff.md

use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{CodebaseAccess, StopReason, SubagentConfig, SubagentRegistry};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// How OpenAI words a context-length refusal, verbatim. Pinned as the real string rather than an
/// invented one: the only surface `OpenAiClient::complete` offers is the error text, so a detector
/// tested against a phrasing no provider emits would pass here and fail in production.
const OPENAI_CONTEXT_REFUSAL: &str = r#"{"error":{"message":"This model's maximum context length is 32768 tokens. However, your messages resulted in 4123 tokens. Please reduce the length of the messages.","type":"invalid_request_error","code":"context_length_exceeded"}}"#;

/// Ollama's wording for the same condition.
const OLLAMA_CONTEXT_REFUSAL: &str =
    r#"{"error":"input length exceeds context length: 41230 > 32768"}"#;

const A_FILE_THE_AGENT_READ: &str = "src/spawn.rs";
const THE_GOAL: &str = "Find where sessions are spawned";

fn a_def(base_url: &str, max_turns: u32) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "qwen2.5-coder:7b".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns,
        replaces: Vec::new(),
    }
}

fn a_local_config() -> SubagentConfig {
    SubagentConfig {
        access: CodebaseAccess::Local,
    }
}

fn a_grep_for(pattern: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Looking for the spawn site.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "GREP",
                        "arguments": serde_json::json!({ "pattern": pattern }).to_string()
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

/// A server whose first answer is a tool call and whose next answer is a context refusal — the
/// shape of a real exhaustion, where the agent has done work before the window fills.
async fn a_model_that_works_then_runs_out_of_context(body: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_grep_for("fn spawn_session")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string(body))
        .mount(&server)
        .await;
    server
}

fn the_brief_of(outcome: &tddy_discovery::subagent::PromptOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn running_out_of_context_is_a_stop_reason_rather_than_a_bare_error() {
    // Given a model that refuses the next turn because the window is full
    let server = a_model_that_works_then_runs_out_of_context(OPENAI_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");

    // When the agent is prompted
    let outcome = session
        .prompt(THE_GOAL)
        .await
        .expect("a full context is a stop condition, not a failure — the caller must get the work");

    // Then it lands the way a spent turn budget already does, under its own reason
    assert_eq!(outcome.stop_reason, StopReason::ContextExhausted);
}

#[tokio::test]
async fn ollamas_wording_for_a_full_context_is_recognised_too() {
    // Given the same condition, worded the way the shipped example's provider words it
    // (`sandbox-config.example.yaml` runs these agents on Ollama at 32k)
    let server = a_model_that_works_then_runs_out_of_context(OLLAMA_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");

    // When
    let outcome = session
        .prompt(THE_GOAL)
        .await
        .expect("the provider's wording must not decide whether the caller gets its work back");

    // Then
    assert_eq!(outcome.stop_reason, StopReason::ContextExhausted);
}

#[tokio::test]
async fn an_unrelated_provider_failure_is_still_an_error() {
    // Given a provider that failed for a reason that is not a full context
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(502).set_body_string("upstream connect timeout"))
        .mount(&server)
        .await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");

    // When
    let result = session.prompt(THE_GOAL).await;

    // Then it stays an error. Detection reads the provider's prose, which is fragile by nature, so
    // it is narrow on purpose: mislabelling a network blip as a full context would send the caller
    // off to rebuild a conversation that had nothing wrong with it.
    assert!(
        result.is_err(),
        "only a context refusal may become a stop reason; a transport failure must not"
    );
}

#[tokio::test]
async fn the_brief_names_the_goal_so_a_new_conversation_knows_what_it_is_for() {
    // Given an agent that ran out of context partway through a goal
    let server = a_model_that_works_then_runs_out_of_context(OPENAI_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");

    // When
    let outcome = session.prompt(THE_GOAL).await.expect("must land, not fail");

    // Then the brief carries the goal verbatim — a replacement conversation starts with no memory
    // whatsoever, so a brief that omits the goal buys nothing
    assert!(
        the_brief_of(&outcome).contains(THE_GOAL),
        "the brief must name the goal; got:\n{}",
        the_brief_of(&outcome)
    );
}

#[tokio::test]
async fn the_brief_indexes_what_was_already_examined_so_the_work_is_not_redone() {
    // Given an agent that had already searched before the window filled
    let server = a_model_that_works_then_runs_out_of_context(OPENAI_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");

    // When
    let outcome = session.prompt(THE_GOAL).await.expect("must land, not fail");
    let brief = the_brief_of(&outcome);

    // Then the search it already ran is named, with its arguments. This is the half that actually
    // saves the work: told only "you ran out of context", a fresh conversation re-greps the same
    // pattern and refills the same window.
    assert!(
        brief.contains("GREP") && brief.contains("fn spawn_session"),
        "the brief must index the tool calls already made, with their arguments; got:\n{brief}"
    );
}

#[tokio::test]
async fn the_brief_keeps_the_findings_and_drops_the_payloads_that_filled_the_window() {
    // Given an agent whose reasoning is in its prose and whose bulk is in its tool results
    let server = a_model_that_works_then_runs_out_of_context(OPENAI_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");

    // When
    let outcome = session.prompt(THE_GOAL).await.expect("must land, not fail");
    let brief = the_brief_of(&outcome);

    // Then what the agent concluded survives — discarding it is precisely what makes a replacement
    // redo the thinking
    assert!(
        brief.contains("Looking for the spawn site."),
        "the brief must keep the assistant's own findings; got:\n{brief}"
    );

    // And the tool output it read does not, because that is what filled the window. A finding
    // derived from a file is worth carrying; the file is not.
    assert!(
        !brief.contains("no matches found") && !brief.contains(A_FILE_THE_AGENT_READ),
        "the brief must drop tool-result bodies; got:\n{brief}"
    );
}

#[tokio::test]
async fn the_brief_tells_the_caller_to_continue_in_a_new_conversation() {
    // Given an exhausted conversation
    let server = a_model_that_works_then_runs_out_of_context(OPENAI_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");

    // When
    let outcome = session.prompt(THE_GOAL).await.expect("must land, not fail");
    let brief = the_brief_of(&outcome);

    // Then the brief says what to do with it. This conversation cannot be continued — its history
    // is still oversized and every further prompt re-sends it — so the instruction has to be
    // explicit, addressed to the agent that will act on it.
    assert!(
        brief.to_lowercase().contains("new conversation"),
        "the brief must tell the caller to open a new conversation; got:\n{brief}"
    );
    assert!(
        brief.to_lowercase().contains("do not repeat")
            || brief.to_lowercase().contains("without repeating"),
        "the brief must say not to redo the examined work; got:\n{brief}"
    );
}

#[tokio::test]
async fn the_session_reports_what_its_history_currently_costs_to_send() {
    // Given a conversation that has run a turn, so it has a history with a size
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "<final_answer>done</final_answer>" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 1200, "completion_tokens": 40 }
        })))
        .mount(&server)
        .await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");
    session.prompt(THE_GOAL).await.expect("the turn must land");

    // When the caller asks how full this conversation is
    let occupancy = session.context_tokens();

    // Then it is what the history costs to send *now* — the last turn's prompt tokens — not the
    // running total. Cumulative input is the sum of growing prefixes, because every turn re-sends
    // the whole history, so a caller watching that number cannot tell a nearly-full conversation
    // from one that has merely run many cheap turns.
    assert_eq!(occupancy, 1200);
}

#[tokio::test]
async fn the_tail_of_an_exhausted_conversation_can_still_be_read() {
    // Given a conversation that ran out of context and can no longer be prompted
    let server = a_model_that_works_then_runs_out_of_context(OPENAI_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");
    session.prompt(THE_GOAL).await.expect("must land, not fail");

    // When the caller reads its tail
    let tail = session.tail(4);

    // Then the last exchanges are there verbatim — the brief is lossy by design, and sometimes the
    // useful thing is exactly where the agent was standing when it stopped. A wedged conversation
    // is unpromptable, not unreadable: it stays in the open map until it is cancelled.
    let rendered = tail.join("\n");
    assert!(
        rendered.contains(THE_GOAL) && rendered.contains("Looking for the spawn site."),
        "the tail must carry the last exchanges as they happened; got:\n{rendered}"
    );
}

#[tokio::test]
async fn the_tail_is_bounded_so_reading_it_cannot_fill_the_callers_own_context() {
    // Given a conversation whose history is longer than the window asked for
    let server = a_model_that_works_then_runs_out_of_context(OPENAI_CONTEXT_REFUSAL).await;
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri(), 6)])
        .create("explorer", a_local_config())
        .expect("the def must resolve");
    session.prompt(THE_GOAL).await.expect("must land, not fail");

    // When only the last two messages are asked for
    let tail = session.tail(2);

    // Then that is what comes back. The caller reading a tail to recover from one full context
    // must not fill its own doing it.
    assert!(
        tail.len() <= 2,
        "the tail must honour the bound it was given; got {} messages",
        tail.len()
    );
}
