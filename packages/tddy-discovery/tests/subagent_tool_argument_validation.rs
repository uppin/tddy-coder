//! Integration and unit tests: a model-issued tool call is checked against the schema the model
//! was given, and an invalid one fails with a breakdown naming every argument that is wrong.
//!
//! Session 01a0e200 spent eight of twenty-four calls on arguments carrying a stray trailing quote
//! — `{"path": "packages/tddy-web/src/lib/terminalGridMeasure.ts\""}`. Five of those were globs,
//! which matched nothing and came back `{"paths": []}` with `is_error: false`: a malformed
//! argument and a pattern that legitimately matches nothing are the same answer today. The three
//! reads came back `file not found`, which is the same answer a real missing file gives, so the
//! agent's correct reading was "wrong path, try another" and it retried the identical broken path
//! twice.
//!
//! The rule these tests pin: a call the schema rejects must never reach the codebase, and the
//! reason must name the argument, the problem and the value — because "file not found" is what
//! sent that session in a circle.
//!
//! Feature: docs/ft/coder/managed-codebase-subagents.md (this behaviour has no section there yet)

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tddy_discovery::agent_def::{SpecializedAgentDef, SubagentTool};
use tddy_discovery::subagent::{
    validate_tool_arguments, ArgumentProblem, ArgumentViolation, CodebaseAccess, MessageRole,
    PromptOutcome, SubagentConfig, SubagentRegistry, SubagentSession,
};
use wiremock::matchers::{method, path as path_matcher};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The exact argument session 01a0e200 issued twice in a row, and the reason this validator
/// exists: a real path, with the model's own closing quote appended to the value.
const A_PATH_WITH_A_TRAILING_QUOTE: &str = "packages/tddy-web/src/lib/terminalGridMeasure.ts\"";

const A_REAL_PATH: &str = "packages/tddy-web/src/lib/scrollFollow.ts";
const THE_GOAL: &str = "Find where the terminal switches to history on scroll";

// ─── Builders ────────────────────────────────────────────────────────────────

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

/// A codebase that answers everything, and records every call that reached it — so an empty
/// recording is proof a rejected call was stopped before the jail rather than after it.
#[derive(Clone)]
struct RecordingCodebase {
    calls: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
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
                  args: serde_json::Value|
                  -> Pin<Box<dyn std::future::Future<Output = String> + Send>> {
                calls.lock().unwrap().push((tool, args));
                Box::pin(async move {
                    serde_json::json!({ "content": "ok", "truncated": false, "total_lines": 1 })
                        .to_string()
                })
            },
        )
    }

    fn tools_reached(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(tool, _)| tool.clone())
            .collect()
    }
}

fn a_turn_calling(tool: &str, arguments: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "Looking.",
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": { "name": tool, "arguments": arguments.to_string() }
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

/// A provider that issues `first_call` once and then answers.
async fn a_model_that_calls_then_answers(first_call: serde_json::Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(first_call))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_matcher("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(a_final_answer("done")))
        .mount(&server)
        .await;
    server
}

fn a_session_over(server: &MockServer, access: CodebaseAccess) -> Box<dyn SubagentSession> {
    SubagentRegistry::from_defs(vec![a_def(&server.uri())])
        .create("explorer", SubagentConfig::new(access))
        .expect("the def must resolve")
}

// ─── Fluent assertions over a violation list ─────────────────────────────────

trait ViolationAssertions {
    fn assert_arguments_faulted(&self, expected: &[&str]) -> &Self;
    fn assert_problem_for(&self, argument: &str, expected: ArgumentProblem) -> &Self;
    fn assert_offending_value_for(&self, argument: &str, expected: &str) -> &Self;
}

impl ViolationAssertions for Vec<ArgumentViolation> {
    fn assert_arguments_faulted(&self, expected: &[&str]) -> &Self {
        let named: Vec<&str> = self.iter().map(|v| v.argument.as_str()).collect();
        assert_eq!(
            named, expected,
            "the breakdown must name exactly the arguments that are wrong, in order"
        );
        self
    }

    fn assert_problem_for(&self, argument: &str, expected: ArgumentProblem) -> &Self {
        let found = self
            .iter()
            .find(|v| v.argument == argument)
            .unwrap_or_else(|| panic!("no violation reported for argument '{argument}'"));
        assert_eq!(
            found.problem, expected,
            "argument '{argument}' was faulted for the wrong reason"
        );
        self
    }

    fn assert_offending_value_for(&self, argument: &str, expected: &str) -> &Self {
        let found = self
            .iter()
            .find(|v| v.argument == argument)
            .unwrap_or_else(|| panic!("no violation reported for argument '{argument}'"));
        assert_eq!(
            found.value.as_deref(),
            Some(expected),
            "the breakdown must quote the value it rejected, so a reader can see the defect"
        );
        self
    }
}

fn the_tool_result_text(outcome: &PromptOutcome) -> String {
    outcome
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("a turn that called a tool appends a result")
        .preview
        .clone()
}

// ─── Main functionality ──────────────────────────────────────────────────────

/// The control: a call the schema accepts produces no breakdown at all.
#[test]
fn a_well_formed_read_call_reports_no_violations() {
    // Given
    let args = serde_json::json!({ "path": A_REAL_PATH, "offset": 0, "limit": 200 });

    // When
    let violations = validate_tool_arguments("READ", &args);

    // Then
    assert_eq!(
        violations,
        Vec::new(),
        "a call matching the advertised schema must pass untouched"
    );
}

/// The defect from session 01a0e200, named rather than guessed at.
#[test]
fn a_path_carrying_a_trailing_quote_is_faulted_and_the_value_is_quoted_back() {
    // Given
    let args = serde_json::json!({ "path": A_PATH_WITH_A_TRAILING_QUOTE });

    // When
    let violations = validate_tool_arguments("READ", &args);

    // Then
    violations
        .assert_arguments_faulted(&["path"])
        .assert_problem_for("path", ArgumentProblem::SurroundingQuote)
        .assert_offending_value_for("path", A_PATH_WITH_A_TRAILING_QUOTE);
}

/// The five silent ones: a glob pattern with the same defect matched nothing and looked like an
/// honest empty result.
#[test]
fn a_glob_pattern_carrying_a_trailing_quote_is_faulted_rather_than_matching_nothing() {
    // Given
    let args = serde_json::json!({ "pattern": "packages/tddy-web/src/**/*view*\"" });

    // When
    let violations = validate_tool_arguments("GLOB", &args);

    // Then
    violations
        .assert_arguments_faulted(&["pattern"])
        .assert_problem_for("pattern", ArgumentProblem::SurroundingQuote);
}

/// A required argument the model left out is named, rather than defaulted to an empty string and
/// dispatched as a read of `""`.
#[test]
fn a_call_missing_a_required_argument_names_the_argument_it_needed() {
    // Given
    let args = serde_json::json!({ "offset": 0 });

    // When
    let violations = validate_tool_arguments("READ", &args);

    // Then
    violations
        .assert_arguments_faulted(&["path"])
        .assert_problem_for("path", ArgumentProblem::Missing);
}

// ─── Edge cases ──────────────────────────────────────────────────────────────

/// Present but blank is its own defect: it is not missing, and reporting it as missing would send
/// the model looking for an argument it did supply.
#[test]
fn a_required_string_that_is_present_but_blank_is_faulted_as_empty() {
    // Given
    let args = serde_json::json!({ "path": "   " });

    // When
    let violations = validate_tool_arguments("READ", &args);

    // Then
    violations
        .assert_arguments_faulted(&["path"])
        .assert_problem_for("path", ArgumentProblem::Empty);
}

/// A regex is not a path. `foo"` is a legal pattern, and faulting it would make GREP unusable for
/// searching quoted source — so the quote rule is scoped to arguments that name files.
#[test]
fn a_grep_regex_ending_in_a_quote_is_accepted_because_a_regex_may_contain_one() {
    // Given
    let args = serde_json::json!({ "pattern": "const NAME = \"" });

    // When
    let violations = validate_tool_arguments("GREP", &args);

    // Then
    assert_eq!(
        violations,
        Vec::new(),
        "the surrounding-quote rule must apply to paths and globs, never to a regex"
    );
}

/// A file body legitimately ends in whatever it ends in, quote included.
#[test]
fn a_write_whose_contents_end_in_a_quote_is_accepted() {
    // Given
    let args = serde_json::json!({ "path": A_REAL_PATH, "contents": "const A = \"b\"" });

    // When
    let violations = validate_tool_arguments("WRITE", &args);

    // Then
    assert_eq!(
        violations,
        Vec::new(),
        "free text is not a path; only path-like arguments carry the quote rule"
    );
}

/// A number where a string belongs is dispatched today as an empty path, because `as_str()`
/// returns `None` and the call site substitutes `""`.
#[test]
fn an_argument_of_the_wrong_type_names_the_type_the_schema_asked_for() {
    // Given
    let args = serde_json::json!({ "path": 42 });

    // When
    let violations = validate_tool_arguments("READ", &args);

    // Then
    violations
        .assert_arguments_faulted(&["path"])
        .assert_problem_for(
            "path",
            ArgumentProblem::WrongType {
                expected: "string".to_string(),
            },
        );
}

/// A typo in an optional argument is silently dropped today, so a model asking for a window gets
/// a whole file and no hint that its spelling was wrong.
#[test]
fn a_misspelled_optional_argument_is_faulted_as_unknown_rather_than_ignored() {
    // Given
    let args = serde_json::json!({ "path": A_REAL_PATH, "limitt": 200 });

    // When
    let violations = validate_tool_arguments("READ", &args);

    // Then
    violations
        .assert_arguments_faulted(&["limitt"])
        .assert_problem_for("limitt", ArgumentProblem::Unknown);
}

/// One round trip per defect would cost a 32k agent its whole budget. The breakdown is the point:
/// everything wrong with the call, in one answer.
#[test]
fn every_fault_in_one_call_is_reported_together_rather_than_only_the_first() {
    // Given
    let args = serde_json::json!({ "path": A_PATH_WITH_A_TRAILING_QUOTE, "limitt": 200 });

    // When
    let violations = validate_tool_arguments("READ", &args);

    // Then
    violations
        .assert_arguments_faulted(&["path", "limitt"])
        .assert_problem_for("path", ArgumentProblem::SurroundingQuote)
        .assert_problem_for("limitt", ArgumentProblem::Unknown);
}

// ─── Error scenarios, through the turn loop ──────────────────────────────────

/// The containment property: a rejected call is rejected *here*, so the jail is never asked to
/// resolve a path that cannot resolve.
#[tokio::test]
async fn a_tool_call_with_an_invalid_argument_never_reaches_the_codebase() {
    // Given a model that reads a path carrying a trailing quote
    let server = a_model_that_calls_then_answers(a_turn_calling(
        "READ",
        serde_json::json!({ "path": A_PATH_WITH_A_TRAILING_QUOTE }),
    ))
    .await;
    let codebase = RecordingCodebase::new();
    let mut session = a_session_over(&server, codebase.access());

    // When the turn runs
    session.prompt(THE_GOAL).await.expect("the turn answers");

    // Then the codebase was never asked
    assert_eq!(
        codebase.tools_reached(),
        Vec::<String>::new(),
        "an argument the schema rejects must be stopped before dispatch, not after"
    );
}

/// What the model is told. `file not found` is what sent session 01a0e200 round the same path
/// twice; the reason has to name the argument and the defect instead.
#[tokio::test]
async fn the_model_is_told_which_argument_was_invalid_and_why() {
    // Given a model that reads a path carrying a trailing quote
    let server = a_model_that_calls_then_answers(a_turn_calling(
        "READ",
        serde_json::json!({ "path": A_PATH_WITH_A_TRAILING_QUOTE }),
    ))
    .await;
    let codebase = RecordingCodebase::new();
    let mut session = a_session_over(&server, codebase.access());

    // When the turn runs
    let outcome = session.prompt(THE_GOAL).await.expect("the turn answers");

    // Then the tool result names the argument and the problem
    let result = the_tool_result_text(&outcome);
    assert!(
        result.contains("path"),
        "the breakdown must name the argument; got: {result}"
    );
    assert!(
        !result.contains("file not found"),
        "a malformed argument must never be reported as a missing file — that is the reading \
         that made the agent retry the same broken path: {result}"
    );
}

/// The main agent's half of the signal: an invalid call is an error in the turn outcome, so a
/// caller watching a subagent can see it failed rather than counting messages going by.
#[tokio::test]
async fn an_invalid_tool_call_is_marked_as_an_error_in_the_turn_outcome() {
    // Given a model that globs a pattern carrying a trailing quote
    let server = a_model_that_calls_then_answers(a_turn_calling(
        "GLOB",
        serde_json::json!({ "pattern": "packages/tddy-web/src/**/*view*\"" }),
    ))
    .await;
    let codebase = RecordingCodebase::new();
    let mut session = a_session_over(&server, codebase.access());

    // When the turn runs
    let outcome = session.prompt(THE_GOAL).await.expect("the turn answers");

    // Then the tool message is flagged
    let tool_message = outcome
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("a turn that called a tool appends a result");
    assert!(
        tool_message.is_error,
        "a glob rejected for a malformed pattern must not read as a glob that matched nothing"
    );
}
