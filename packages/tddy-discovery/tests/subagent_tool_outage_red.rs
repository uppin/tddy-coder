//! Integration tests: what a specialized agent answers when its tools are dead.
//!
//! On 2026-09-26 a jailed session's tool channel closed. FastContext was consulted twelve minutes
//! later and made 54 tool calls, **every one refused**. It read nothing. It then spent its whole
//! turn budget and was handed `run_synthesis_turn`'s instruction — *"Summarize your findings now
//! from what you have already read, citing the specific file:line locations you found."* — which
//! does not check that anything was found. It answered with a line number, a constant value, a
//! working directory and a tool list it had invented, and the main agent very nearly changed code
//! on the strength of it.
//!
//! Turn exhaustion and context exhaustion both land softly, and should: they describe a search
//! that happened and ran out. A total tool outage describes a search that never started, and it
//! has no landing at all today. It gets an error instead of an answer — and the error must be
//! reached *without* a synthesis model call, because a model asked to cite what it never read will
//! oblige.
//!
//! The load-bearing assertion in this file is a **request count**. "No synthesis happened" cannot
//! be checked by looking at the answer: a synthesis that ran and returned something error-shaped
//! would pass that. It can only be checked by counting what the provider was asked.
//!
//! Changeset: docs/dev/1-WIP/2026-09-26-subagent-turn-control-and-honest-tool-failure.md

use std::pin::Pin;
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, PromptOutcome, StopReason, SubagentConfig, SubagentRegistry, SubagentSession,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The text the jail itself produces when its tool channel has closed, verbatim from
/// `tddy_daemon_sandbox::workspace_tool_sandbox`. Pinned as the real string rather than an
/// invented one: the whole point of the change is that *this* reaches the caller, and a test
/// written against a phrasing nothing emits would pass here and prove nothing in production.
const THE_JAIL_IS_DEAD: &str = "session 01a0dd54-71ad-7422-865e-d3a0b994d57d: the tool call could \
                                not be run in its jail (its channel is closed); refusing to run \
                                it on the host worktree instead";

/// Small enough to keep the suite quick, large enough that "budget + 1" is unmistakable.
const A_SHORT_BUDGET: u32 = 4;

const THE_GOAL: &str = "Find where the terminal's scrollback size is set";

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

/// A codebase whose every tool call is refused by the transport, the way a closed jail channel
/// refuses one. `is_error: true` is the envelope `CodebaseAccess::parse_dispatch_result` reads.
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

/// A codebase whose tool calls succeed, for the contrast case.
fn a_codebase_that_answers() -> CodebaseAccess {
    CodebaseAccess::managed(
        |_tool: String,
         _args: serde_json::Value|
         -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            Box::pin(async move {
                serde_json::json!({
                    "content": "const PAGE_SCROLLBACK = 50000;",
                    "truncated": false,
                    "total_lines": 1,
                })
                .to_string()
            })
        },
    )
}

/// A model that only ever reaches for a tool — so the loop is bounded by its budget, never by the
/// model deciding it is finished.
fn a_turn_that_reads(path: &str) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Looking at the terminal component.",
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

/// The fabrication the incident actually produced, so a synthesis turn that *does* run is caught
/// by what it says and not only by the request count.
fn a_confident_summary_of_nothing() -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "GhosttyTerminalSession.tsx:108 — const PAGE_SCROLLBACK = 10000;"
            },
            "finish_reason": "stop"
        }]
    })
}

/// A provider that answers every turn with a tool call, and would answer a tool-less synthesis
/// turn with a citation it has no basis for.
async fn a_model_that_keeps_reaching_for_tools() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(a_turn_that_reads("src/terminal.tsx")),
        )
        .up_to_n_times(u64::from(A_SHORT_BUDGET))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_confident_summary_of_nothing()))
        .mount(&server)
        .await;
    server
}

async fn turns_the_model_was_asked_for(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("the mock provider records its requests")
        .len()
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

fn the_answer_of(outcome: &PromptOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// The headline. A prompt whose every tool call was refused reports the refusal.
#[tokio::test]
async fn a_prompt_whose_every_tool_call_failed_reports_the_transport_failure() {
    // Given an agent whose jail is dead, and a model that keeps reaching for tools
    let server = a_model_that_keeps_reaching_for_tools().await;
    let mut session = a_session_over(&server, a_codebase_whose_jail_is_dead(), A_SHORT_BUDGET);

    // When it is asked to explore
    let result = session.prompt(THE_GOAL).await;

    // Then it answers with the jail's own words rather than with a summary
    let error = result
        .err()
        .map(|e| e.to_string())
        .unwrap_or_else(|| panic!("a search in which nothing could be read is not an answer"));
    assert!(
        error.contains("its channel is closed"),
        "the caller must be told the channel died, not merely that a turn failed; got: {error}"
    );
}

/// The structural guarantee behind the one above: nothing was *asked* to summarize.
///
/// A budget of N spent entirely on failing tool calls must cost exactly N model turns. The
/// N+1'th would be `run_synthesis_turn`, and its absence is the only thing that makes the
/// fabrication impossible rather than merely unlikely.
#[tokio::test]
async fn a_total_tool_outage_never_reaches_a_synthesis_turn() {
    // Given the same dead jail
    let server = a_model_that_keeps_reaching_for_tools().await;
    let mut session = a_session_over(&server, a_codebase_whose_jail_is_dead(), A_SHORT_BUDGET);

    // When the whole budget is spent on refused tool calls
    let _ = session.prompt(THE_GOAL).await;

    // Then the model was asked for the budget and not one turn more
    assert_eq!(
        turns_the_model_was_asked_for(&server).await,
        A_SHORT_BUDGET as usize,
        "one extra request is the synthesis turn — the turn that invents a citation"
    );
}

/// The regression guard for the path that is *not* changing. An agent that read something and ran
/// out of turns still lands softly, because that describes a search that happened.
#[tokio::test]
async fn a_prompt_with_working_tools_still_lands_in_a_synthesis_summary() {
    // Given an agent whose tools answer, and a model that never stops reaching for them
    let server = a_model_that_keeps_reaching_for_tools().await;
    let mut session = a_session_over(&server, a_codebase_that_answers(), A_SHORT_BUDGET);

    // When the budget runs out
    let outcome = session
        .prompt(THE_GOAL)
        .await
        .expect("a search that read something still answers");

    // Then it lands exactly as it does today
    assert_eq!(outcome.stop_reason, StopReason::MaxTurnRequests);
    assert!(
        !the_answer_of(&outcome).is_empty(),
        "the soft landing must still carry what was gathered"
    );
    assert_eq!(
        turns_the_model_was_asked_for(&server).await,
        A_SHORT_BUDGET as usize + 1,
        "the budget plus the one synthesis turn"
    );
}

/// A single failing tool call among successful ones is an ordinary result, not an outage. Getting
/// this wrong would turn every missing file into a hard error.
#[tokio::test]
async fn one_failed_tool_call_among_successful_ones_is_not_an_outage() {
    // Given a codebase that refuses only its first call
    let server = a_model_that_keeps_reaching_for_tools().await;
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = std::sync::Arc::clone(&calls);
    let access = CodebaseAccess::managed(
        move |_tool: String,
              _args: serde_json::Value|
              -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
            let n = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move {
                if n == 0 {
                    serde_json::json!({ "is_error": true, "error": "READ src/gone.rs: No such file or directory" }).to_string()
                } else {
                    serde_json::json!({ "content": "fn main() {}", "truncated": false, "total_lines": 1 }).to_string()
                }
            })
        },
    );
    let mut session = a_session_over(&server, access, A_SHORT_BUDGET);

    // When the agent explores and runs out of turns
    let outcome = session
        .prompt(THE_GOAL)
        .await
        .expect("one missing file is not a tool outage");

    // Then it lands softly, because the search did happen
    assert_eq!(outcome.stop_reason, StopReason::MaxTurnRequests);
}
