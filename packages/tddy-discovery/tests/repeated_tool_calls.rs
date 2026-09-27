//! Integration tests: a subagent that asks the same question a third time is stopped and told so.
//!
//! Two agents looped in session `01a0e285`, in opposite directions, and neither was caught.
//!
//! **FastContext** made 23 tool calls, 19 of them `READ`, across **three** distinct argument sets
//! — `GhosttyTerminalSession.tsx` nine times and `scrollFollow.ts` eight, byte-identical each
//! time. Every call succeeded and returned the same bytes, so the existing guard
//! (`ToolCallTally`, which fires only when *nothing* ran) could not see it. Per-turn latency
//! climbed 6s → 168s as the history refilled with copies of what it already had.
//!
//! **Gemma** repeated a *failing* call four times: `StrReplace` with `old_string: "\n\n"`,
//! answered `old_string matches 51 times (must be unique)` every time. That error names the
//! problem, the count and the rule — it is about as good as an error message gets, and the agent
//! repeated the call anyway. So this is not a diagnostics gap; it is an agent that has stopped
//! responding to information, and only the caller can break the cycle.
//!
//! The predicate is therefore "made no progress", not "everything failed" — which is the
//! generalisation of the guard that already exists.
//!
//! **A repeat is not always a mistake**, and the ledger has to know the difference: re-reading a
//! file *after writing to it* is the correct thing to do. So a mutation that **succeeded** clears
//! the ledger, and one that **failed** does not — the second being exactly Gemma's case, where
//! four identical refusals changed nothing about the worktree.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    CodebaseAccess, MessageRole, RepeatedCalls, SubagentConfig, SubagentRegistry,
    IDENTICAL_CALL_LIMIT,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The two calls FastContext made nine and eight times.
const A_READ: &str = r#"{"path":"packages/tddy-web/src/components/GhosttyTerminalSession.tsx"}"#;
const ANOTHER_READ: &str = r#"{"path":"packages/tddy-web/src/lib/scrollFollow.ts"}"#;
/// The one Gemma made four times, and was refused four times.
const A_DOOMED_REPLACE: &str = r#"{"path":"a.tsx","old_string":"\n\n","new_string":""}"#;

const THE_GOAL: &str = "Where does the terminal switch to history on scroll?";

// ─── The ledger, on its own ──────────────────────────────────────────────────

/// The common case must cost nothing: a call nobody has made goes straight through.
#[test]
fn the_first_call_of_its_kind_is_admitted() {
    // Given an empty ledger
    let mut ledger = RepeatedCalls::new();

    // When a call is made for the first time
    let admitted = ledger.admit("READ", A_READ);

    // Then it is allowed
    assert_eq!(admitted, Ok(()));
}

/// Asking twice is not a loop. A model that re-reads to confirm something is working, and a
/// guard that fires on the second call would break a legitimate habit.
#[test]
fn a_second_identical_call_is_admitted_because_asking_twice_is_not_a_loop() {
    // Given a call already made once
    let mut ledger = RepeatedCalls::new();
    ledger.admit("READ", A_READ).expect("the first is allowed");

    // When it is made again
    let admitted = ledger.admit("READ", A_READ);

    // Then it is still allowed
    assert_eq!(admitted, Ok(()));
}

/// The third is the one that never had a different answer coming.
#[test]
fn a_third_identical_call_is_refused() {
    // Given a call already made twice
    let mut ledger = RepeatedCalls::new();
    ledger.admit("READ", A_READ).expect("the first is allowed");
    ledger.admit("READ", A_READ).expect("the second is allowed");

    // When it is made a third time
    let refused = ledger.admit("READ", A_READ);

    // Then it is refused
    assert!(
        refused.is_err(),
        "a call whose answer cannot have changed must not be dispatched again"
    );
}

/// The refusal is the only thing the model will read, so it has to say what is wrong rather than
/// simply failing — otherwise it is one more uninformative error to loop on.
#[test]
fn the_refusal_names_the_tool_and_how_many_times_the_call_was_made() {
    // Given a call already made twice
    let mut ledger = RepeatedCalls::new();
    ledger.admit("READ", A_READ).expect("the first is allowed");
    ledger.admit("READ", A_READ).expect("the second is allowed");

    // When it is made a third time
    let refusal = ledger
        .admit("READ", A_READ)
        .expect_err("a third identical call is refused");

    // Then the message names the tool and the count
    let text = refusal.to_string();
    assert!(
        text.contains("READ") && text.contains(&IDENTICAL_CALL_LIMIT.to_string()),
        "the refusal must name the tool and how often it was called; got: {text}"
    );
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// The ledger is per call, not per tool: a search that is actually moving must never be stopped
/// because a different one stalled.
#[test]
fn a_call_with_different_arguments_is_admitted_however_often_its_neighbour_was_made() {
    // Given one call made to its limit
    let mut ledger = RepeatedCalls::new();
    ledger.admit("READ", A_READ).expect("first");
    ledger.admit("READ", A_READ).expect("second");

    // When a different file is read
    let admitted = ledger.admit("READ", ANOTHER_READ);

    // Then it is allowed
    assert_eq!(admitted, Ok(()));
}

/// Same arguments, different tool, different question.
#[test]
fn the_same_arguments_sent_to_a_different_tool_are_a_different_call() {
    // Given a READ made to its limit
    let mut ledger = RepeatedCalls::new();
    ledger.admit("READ", A_READ).expect("first");
    ledger.admit("READ", A_READ).expect("second");

    // When the same arguments go to another tool
    let admitted = ledger.admit("READ_LINTS", A_READ);

    // Then it is allowed
    assert_eq!(admitted, Ok(()));
}

/// Re-reading a file you have just written is correct, and must stay possible. A mutation that
/// landed makes every earlier answer stale, so the ledger starts over.
#[test]
fn a_mutation_that_succeeded_clears_the_ledger_so_the_same_read_can_be_made_again() {
    // Given a read made to its limit, and then a write that landed
    let mut ledger = RepeatedCalls::new();
    ledger.admit("READ", A_READ).expect("first");
    ledger.admit("READ", A_READ).expect("second");
    ledger.record_outcome("STR_REPLACE", true);

    // When the file is read again
    let admitted = ledger.admit("READ", A_READ);

    // Then it is allowed, because the file is not what it was
    assert_eq!(admitted, Ok(()));
}

/// Gemma's case, and the reason the reset is keyed on success rather than on the attempt: four
/// refused `StrReplace` calls changed nothing, so nothing about the worktree was new and the
/// ledger must still be standing.
#[test]
fn a_mutation_that_failed_leaves_the_ledger_standing() {
    // Given a replace that was refused twice, changing nothing
    let mut ledger = RepeatedCalls::new();
    ledger
        .admit("STR_REPLACE", A_DOOMED_REPLACE)
        .expect("first");
    ledger.record_outcome("STR_REPLACE", false);
    ledger
        .admit("STR_REPLACE", A_DOOMED_REPLACE)
        .expect("second");
    ledger.record_outcome("STR_REPLACE", false);

    // When it is attempted a third time
    let refused = ledger.admit("STR_REPLACE", A_DOOMED_REPLACE);

    // Then it is refused: a failed write is not news
    assert!(
        refused.is_err(),
        "a mutation that failed changed nothing, so repeating it cannot succeed either"
    );
}

/// A read does not change the worktree, so finishing one must not wipe the memory of it.
#[test]
fn a_successful_read_does_not_clear_the_ledger() {
    // Given two identical reads, each of which succeeded
    let mut ledger = RepeatedCalls::new();
    ledger.admit("READ", A_READ).expect("first");
    ledger.record_outcome("READ", true);
    ledger.admit("READ", A_READ).expect("second");
    ledger.record_outcome("READ", true);

    // When the same read is attempted again
    let refused = ledger.admit("READ", A_READ);

    // Then it is refused
    assert!(
        refused.is_err(),
        "only a mutation makes an earlier answer stale; a read that worked changes nothing"
    );
}

// ─── Through the turn loop ───────────────────────────────────────────────────

#[derive(Clone)]
struct RecordingCodebase {
    calls: Arc<Mutex<Vec<String>>>,
}

impl RecordingCodebase {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn access(&self) -> CodebaseAccess {
        let calls = Arc::clone(&self.calls);
        CodebaseAccess::managed(
            move |tool: String,
                  _args: serde_json::Value|
                  -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
                calls.lock().unwrap().push(tool);
                Box::pin(async move {
                    serde_json::json!({ "content": "x", "truncated": false, "total_lines": 1 })
                        .to_string()
                })
            },
        )
    }

    fn times_reached(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

fn a_def(base_url: &str) -> SpecializedAgentDef {
    SpecializedAgentDef {
        name: "explorer".to_string(),
        label: None,
        model: "fastcontext-tools-32k:latest".to_string(),
        base_url: base_url.to_string(),
        api_key: None,
        system_prompt: None,
        system_prompt_path: None,
        tools: vec![SubagentTool::Read, SubagentTool::Glob, SubagentTool::Grep],
        max_turns: 2,
        replaces: Vec::new(),
    }
}

/// One assistant message that reads the same file three times over, which is the smallest thing
/// that reproduces the shape of FastContext's nine.
fn a_turn_that_reads_the_same_file_three_times() -> serde_json::Value {
    let call = |id: &str| {
        serde_json::json!({
            "id": id,
            "type": "function",
            "function": { "name": "READ", "arguments": A_READ }
        })
    };
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Reading.",
                "tool_calls": [call("c1"), call("c2"), call("c3")]
            },
            "finish_reason": "tool_calls"
        }]
    })
}

async fn a_model_that_loops_then_answers() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(a_turn_that_reads_the_same_file_three_times()),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": "done" },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;
    server
}

/// The containment property: the third identical call is stopped here, so the jail is never asked
/// to fetch bytes the conversation already holds.
#[tokio::test]
async fn a_repeated_call_is_refused_before_the_codebase_is_asked_again() {
    // Given a model that reads the same file three times in one turn
    let server = a_model_that_loops_then_answers().await;
    let codebase = RecordingCodebase::new();
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create("explorer", SubagentConfig::new(codebase.access()))
        .expect("the def must resolve");

    // When the turn runs
    session.prompt(THE_GOAL).await.expect("the turn answers");

    // Then the codebase served only the calls that could still say something new
    assert_eq!(
        codebase.times_reached(),
        IDENTICAL_CALL_LIMIT - 1,
        "the third identical call must not reach the jail: it would return bytes the \
         conversation already holds, and pay a turn of context for them"
    );
}

/// The caller's half of the signal: a refused repeat is an error in the turn outcome, so a main
/// agent watching a subagent can see it going in circles instead of counting messages go by.
#[tokio::test]
async fn a_refused_repeat_is_reported_as_an_error_in_the_turn_outcome() {
    // Given a model that reads the same file three times in one turn
    let server = a_model_that_loops_then_answers().await;
    let codebase = RecordingCodebase::new();
    let mut session = SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create("explorer", SubagentConfig::new(codebase.access()))
        .expect("the def must resolve");

    // When the turn runs
    let outcome = session.prompt(THE_GOAL).await.expect("the turn answers");

    // Then exactly one of the three tool results is flagged
    let flagged = outcome
        .messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool && m.is_error)
        .count();
    assert_eq!(
        flagged, 1,
        "the two calls that ran are results; the one that was refused is an error"
    );
}
