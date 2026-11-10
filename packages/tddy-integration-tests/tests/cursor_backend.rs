//! Acceptance tests for the `cursor` spec turn (`SpecHeadlessBackend` over a stub `agent`).
//!
//! Verifies that the generic spec backend spawns the `agent` CLI with correct flags, parses
//! Cursor's stream-json output, and captures the thread id for --resume (`#agent-tools 15/19`
//! replaced the bespoke `CursorBackend`). Cursor writes `.cursor/mcp.json` into the request's
//! working directory, so each turn runs in the fixture's git checkout.
//! Migrated from Workflow to WorkflowEngine where applicable.

mod common;

use std::fs;
use std::sync::Arc;
use tddy_core::workflow::graph::ExecutionStatus;
use tddy_core::{
    BackendError, CodingBackend, GoalId, InvokeRequest, InvokeResponse, SharedBackend,
};

use common::{
    a_spec_backend, ctx_plan, in_spec_workspace, session_dir_for_new_session, spec_argv,
    stub_invoke_request,
};

/// Runs `req` over a stub `agent` that prints `stdout_lines` (shell lines) after recording its argv.
fn run_cursor(
    stub_script: &str,
    req: InvokeRequest,
) -> (Result<InvokeResponse, BackendError>, tempfile::TempDir) {
    let (backend, dir) = a_spec_backend("cursor", stub_script);
    let req = in_spec_workspace(&dir, req);
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(backend.invoke(req));
    (result, dir)
}

/// CursorBackend spawns agent with -p, --output-format stream-json, --force, --trust.
#[test]
fn cursor_backend_spawns_cursor_agent_with_correct_flags() {
    // Given
    let script = r##"printf '%s\n' '{"type":"system","thread_id":"cursor-thread-abc"}'
printf '%s\n' '{"type":"result","result":"output","session_id":"cursor-thread-abc"}'
exit 0"##;

    // When
    let (result, dir) = run_cursor(script, stub_invoke_request("test prompt", "plan"));
    let result = result.expect("invoke should succeed");

    // Then
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, "output");
    assert_eq!(result.session_id.as_deref(), Some("cursor-thread-abc"));

    let argv = spec_argv(&dir);
    assert_eq!(
        argv.first().map(String::as_str),
        Some("--plan"),
        "Plan goal should pass --plan as first arg, got: {argv:?}"
    );
    for flag in [
        "-p",
        "--output-format",
        "stream-json",
        "--force",
        "--trust",
        "--plan",
    ] {
        assert!(
            argv.iter().any(|a| a == flag),
            "should have {flag}, got: {argv:?}"
        );
    }
}

/// CursorBackend does not pass --plan when goal is not Plan.
#[test]
fn cursor_backend_omits_plan_flag_when_goal_is_not_plan() {
    for goal_id in ["acceptance-tests", "red", "green"] {
        // Given
        let script = r##"printf '%s\n' '{"type":"result","result":"ok","session_id":"s1"}'"##;

        // When
        let (result, dir) = run_cursor(script, stub_invoke_request("Create tests", goal_id));
        result.expect("invoke should succeed");

        // Then
        let argv = spec_argv(&dir);
        assert!(
            !argv.iter().any(|a| a == "--plan"),
            "should not have --plan when goal is {goal_id}, got: {argv:?}"
        );
    }
}

/// Validate uses the same `agent` CLI path as other non-plan goals (parity with Claude).
#[test]
fn cursor_backend_invokes_validate_goal_like_other_non_plan_goals() {
    // Given
    let script =
        r##"printf '%s\n' '{"type":"result","result":"validate-ok","session_id":"val-sid"}'"##;

    // When
    let (result, dir) = run_cursor(
        script,
        stub_invoke_request("run validate subagents", "validate"),
    );
    let result = result.expect("invoke Goal::Validate must succeed like other goals");

    // Then
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, "validate-ok");
    assert_eq!(result.session_id.as_deref(), Some("val-sid"));
    let argv = spec_argv(&dir);
    assert!(
        !argv.iter().any(|a| a == "--plan"),
        "validate must not use --plan, got: {argv:?}"
    );
}

/// Refactor uses the same `agent` CLI path as other non-plan goals (parity with Claude).
#[test]
fn cursor_backend_invokes_refactor_goal_like_other_non_plan_goals() {
    // Given
    let script =
        r##"printf '%s\n' '{"type":"result","result":"refactor-ok","session_id":"ref-sid"}'"##;

    // When
    let (result, dir) = run_cursor(
        script,
        stub_invoke_request("apply refactoring plan", "refactor"),
    );
    let result = result.expect("invoke Goal::Refactor must succeed like other non-plan goals");

    // Then
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, "refactor-ok");
    assert_eq!(result.session_id.as_deref(), Some("ref-sid"));
    let argv = spec_argv(&dir);
    assert!(
        !argv.iter().any(|a| a == "--plan"),
        "refactor must not use --plan, got: {argv:?}"
    );
}

/// Cursor `agent` does not accept `--session-id` for new chats; Fresh must not pass it.
#[test]
fn cursor_backend_omits_session_id_on_fresh_session() {
    // Given
    let script = r##"printf '%s\n' '{"type":"result","result":"ok","session_id":"from-stream"}'"##;
    let mut req = stub_invoke_request("first turn", "plan");
    req.session = Some(tddy_core::SessionMode::Fresh(
        "7a552e2e-1d03-4f9b-82fa-f0c45b343ae1".to_string(),
    ));

    // When
    let (result, dir) = run_cursor(script, req);
    let result = result.expect("invoke should succeed");

    // Then
    assert_eq!(result.session_id.as_deref(), Some("from-stream"));
    let argv = spec_argv(&dir);
    assert!(
        !argv.iter().any(|a| a == "--session-id"),
        "Cursor CLI has no --session-id; must not pass it on Fresh, got: {argv:?}"
    );
    assert!(
        !argv.iter().any(|a| a == "--resume"),
        "Fresh session must not use --resume, got: {argv:?}"
    );
    assert!(
        !argv.iter().any(|a| a.contains("7a552e2e")),
        "a fresh session passes no session flag, got: {argv:?}"
    );
}

/// CursorBackend adds --resume when session is SessionMode::Resume.
#[test]
fn cursor_backend_adds_resume_flag_on_followup() {
    // Given
    let script = r##"printf '%s\n' '{"type":"result","result":"continued"}'"##;
    let mut req = stub_invoke_request("continue", "plan");
    req.session = Some(tddy_core::SessionMode::Resume("prev-thread-id".to_string()));

    // When
    let (result, dir) = run_cursor(script, req);
    result.expect("invoke should succeed");

    // Then
    let argv = spec_argv(&dir);
    let at = argv
        .iter()
        .position(|a| a == "--resume")
        .expect("--resume on followup");
    assert_eq!(argv.get(at + 1).map(String::as_str), Some("prev-thread-id"));
}

/// The backend is named for its spec.
#[test]
fn cursor_backend_name_returns_cursor() {
    // Given
    let (backend, _dir) = a_spec_backend("cursor", "exit 0");

    // Then
    assert_eq!(backend.name(), "cursor");
}

/// Extract the prompt value passed to -p from the recorded argv (one entry per line).
/// The prompt may span multiple lines when it contains system + user content.
fn prompt_from_argv(argv: &[String]) -> Option<String> {
    let i = argv.iter().position(|l| l == "-p")?;
    let start = i + 1;
    // The prompt runs until the next flag (--output-format)
    let end = argv[start..]
        .iter()
        .position(|l| l == "--output-format")
        .map(|j| start + j)
        .unwrap_or(argv.len());
    Some(argv[start..end].join("\n"))
}

const OK_RESULT: &str = r##"printf '%s\n' '{"type":"result","result":"ok","session_id":"s1"}'"##;

/// An inline system_prompt is merged into the prompt passed to -p (system, blank line, user).
/// Cursor CLI has no --system-prompt; system instructions are prepended to the user prompt.
#[test]
fn cursor_backend_includes_system_prompt_in_prompt_when_set() {
    // Given
    let mut req = stub_invoke_request("Create a PRD for: Add login", "plan");
    req.system_prompt = Some("You MUST output a <structured-response> block.".to_string());

    // When
    let (result, dir) = run_cursor(OK_RESULT, req);
    result.expect("invoke should succeed");

    // Then
    let prompt = prompt_from_argv(&spec_argv(&dir)).expect("prompt should be present after -p");
    assert_eq!(
        prompt,
        "You MUST output a <structured-response> block.\n\nCreate a PRD for: Add login"
    );
}

/// The content of a system_prompt_path file is merged into the prompt when set.
#[test]
fn cursor_backend_includes_system_prompt_path_content_in_prompt_when_set() {
    // Given
    let files = tempfile::tempdir().expect("dir");
    let system_file = std::fs::canonicalize(files.path())
        .unwrap()
        .join("system.md");
    fs::write(&system_file, "Output format: structured-response only.").expect("write system file");
    let mut req = stub_invoke_request("Plan: Add logout", "plan");
    req.system_prompt_path = Some(system_file);

    // When
    let (result, dir) = run_cursor(OK_RESULT, req);
    result.expect("invoke should succeed");

    // Then
    let prompt = prompt_from_argv(&spec_argv(&dir)).expect("prompt should be present after -p");
    assert_eq!(
        prompt,
        "Output format: structured-response only.\n\nPlan: Add logout"
    );
}

/// system_prompt_path wins over an inline system_prompt when both are set.
#[test]
fn cursor_backend_prefers_system_prompt_path_over_system_prompt() {
    // Given
    let files = tempfile::tempdir().expect("dir");
    let system_file = std::fs::canonicalize(files.path())
        .unwrap()
        .join("system.md");
    fs::write(&system_file, "From file: use structured-response.").expect("write system file");
    let mut req = stub_invoke_request("Plan feature", "plan");
    req.system_prompt = Some("Inline: ignored when path set.".to_string());
    req.system_prompt_path = Some(system_file);

    // When
    let (result, dir) = run_cursor(OK_RESULT, req);
    result.expect("invoke should succeed");

    // Then
    let prompt = prompt_from_argv(&spec_argv(&dir)).expect("prompt should be present after -p");
    assert!(
        prompt.contains("From file: use structured-response."),
        "prompt should use system_prompt_path content, got: {prompt}"
    );
    assert!(
        !prompt.contains("Inline: ignored when path set."),
        "prompt should not use system_prompt when path is set, got: {prompt}"
    );
}

/// The agent is handed the MCP registration in the checkout, which the turn removes afterwards.
#[test]
fn cursor_turn_registers_the_tool_server_in_the_checkout_and_removes_it_after() {
    // Given
    let script =
        format!("[ -f .cursor/mcp.json ] && cp .cursor/mcp.json ../mcp-seen.json\n{OK_RESULT}");

    // When
    let (result, dir) = run_cursor(&script, stub_invoke_request("hi", "plan"));
    result.expect("invoke should succeed");

    // Then
    let root = common::spec_workspace_root(&dir);
    let seen =
        fs::read_to_string(root.join("mcp-seen.json")).expect("the agent saw .cursor/mcp.json");
    assert!(seen.contains("tddy-tools"), "{seen}");
    assert!(
        !root.join("repo/.cursor/mcp.json").exists(),
        "the registration is removed after the turn"
    );
}

/// The cursor spec turn in a plan workflow returns ClarificationNeeded when stream contains
/// clarification-questions block in result event.
/// IGNORED: inline clarification-questions text parsing was removed (Drop Inline Parsing plan).
/// Clarification now comes from AskUserQuestion tool events or tddy-tools ask.
#[tokio::test]
#[cfg(unix)]
#[ignore = "inline clarification-questions parsing removed; use tool events or tddy-tools ask"]
async fn cursor_backend_plan_returns_clarification_needed_when_stream_has_questions() {
    // Given
    let tmp = std::env::temp_dir().join("tddy-cursor-clarification-plan");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    let result_content = "Before I produce the PRD, I have a few clarification points.\n\n<clarification-questions content-type=\"application-json\">\n{\"questions\":[{\"header\":\"Validate goal scope\",\"question\":\"Replace or merge?\",\"options\":[{\"label\":\"Replace only\",\"description\":\"validate = renamed validate-refactor\"},{\"label\":\"Merge both\",\"description\":\"validate replaces both\"}],\"multiSelect\":false},{\"header\":\"Refactor behavior\",\"question\":\"TDD or direct?\",\"options\":[{\"label\":\"TDD\",\"description\":\"red→green\"},{\"label\":\"Direct\",\"description\":\"direct apply\"}],\"multiSelect\":false}]}\n</clarification-questions>";
    let ndjson = format!(
        r#"{{"type":"system","subtype":"init","session_id":"cursor-sess"}}
{{"type":"result","subtype":"success","is_error":false,"result":{},"session_id":"cursor-sess"}}"#,
        serde_json::to_string(result_content).expect("escape")
    );
    let tmp_canonical = tmp.canonicalize().unwrap_or_else(|_| tmp.clone());
    let stream_path = tmp_canonical.join("stream.ndjson");
    fs::write(&stream_path, ndjson).expect("write stream");

    let (backend, _workspace) =
        a_spec_backend("cursor", &format!("cat \"{}\"", stream_path.display()));
    let backend = Arc::new(backend);
    let storage_dir = std::env::temp_dir().join("tddy-cursor-clarification-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = common::tdd_engine(SharedBackend::from_arc(backend), storage_dir);

    let session_dir = session_dir_for_new_session();
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    let ctx = ctx_plan("Feature X", tmp.clone(), session_dir.clone(), None, None);
    let plan_gid = GoalId::new("plan");

    // When
    let result = engine.run_goal(&plan_gid, ctx).await.expect("run_goal");

    match &result.status {
        ExecutionStatus::WaitingForInput { .. } => {
            // Then
            assert!(!result.session_id.is_empty(), "session_id should be set");
        }
        ExecutionStatus::Completed => {
            panic!("expected WaitingForInput (ClarificationNeeded), got Completed")
        }
        ExecutionStatus::Error(e) => panic!("expected WaitingForInput, got Error: {}", e),
        ExecutionStatus::Paused { .. } => panic!("expected WaitingForInput, got Paused"),
        ExecutionStatus::ElicitationNeeded { .. } => {
            panic!("expected WaitingForInput, got ElicitationNeeded")
        }
    }

    let _ = std::fs::remove_dir_all(&tmp);
}
