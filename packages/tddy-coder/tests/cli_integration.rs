//! Integration tests for CLI argument parsing and stdin.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use std::fs;
use tddy_core::output::{SESSIONS_SUBDIR, TDDY_SESSIONS_DIR_ENV};

fn tddy_coder_bin() -> Command {
    cargo_bin_cmd!("tddy-coder")
}

/// When --goal is omitted, the full workflow (plan -> acceptance-tests -> red -> green) runs.
#[test]
#[cfg(unix)]
fn cli_runs_full_workflow_when_goal_omitted() {
    let tmp = std::env::temp_dir().join("tddy-cli-full-workflow-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args(["--agent", "stub", "--prompt", "SKIP_QUESTIONS Build auth"])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("the following required arguments were not provided")
            && !stderr.contains("--goal"),
        "when --goal is omitted, full workflow should run (not require --goal). stderr: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(unix)]
fn cli_accepts_goal_plan() {
    let tmp = std::env::temp_dir().join("tddy-cli-goal-test");
    let _ = std::fs::create_dir_all(&tmp);

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "SKIP_QUESTIONS Build feature X",
        ])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unrecognized") && !stderr.contains("unknown"),
        "should not fail on arg parsing: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Plain mode plan approval: after plan completes, user must approve. Piping "a" approves.
#[test]
#[cfg(unix)]
fn cli_plain_mode_plan_approval_approve_proceeds() {
    let tmp = std::env::temp_dir().join("tddy-cli-plan-approval-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "SKIP_QUESTIONS Build feature for approval test",
        ])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "expected success: stdout={} stderr={}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("Plan generated") || stderr.contains("Plan generated"),
        "expected plan approval prompt: stdout={} stderr={}",
        stdout,
        stderr
    );
    let last_line = stdout.trim().lines().last().unwrap_or("").trim();
    let session_dir = std::path::Path::new(last_line);
    let prd = session_dir.join("artifacts").join("PRD.md");
    assert!(
        session_dir.is_dir() && prd.exists(),
        "stdout should end with plan dir path with artifacts/PRD.md, last_line={} stdout={}",
        last_line,
        stdout
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Each goal should log the agent and model it is using before execution.
/// Uses log config with file output to collect log entries.
#[test]
#[cfg(unix)]
fn cli_displays_agent_and_model_before_goal_execution() {
    let tmp = std::env::temp_dir().join("tddy-cli-agent-model-display");
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::create_dir_all(&tmp);

    let log_file = tmp.join("debug.log");
    let config_yaml = format!(
        r#"log:
  loggers:
    default:
      output: {{ file: "{}" }}
      format: "{{timestamp}} [{{level}}] [{{target}}] {{message}}"
  default:
    level: debug
    logger: default
"#,
        log_file.display()
    );
    let config_path = tmp.join("config.yaml");
    std::fs::write(&config_path, config_yaml).expect("write config");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "-c",
            config_path.to_str().unwrap(),
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "SKIP_QUESTIONS Build feature X",
        ])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");
    assert!(
        output.status.success(),
        "CLI should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let logs = fs::read_to_string(&log_file).unwrap_or_default();
    assert!(
        logs.contains("agent") && logs.contains("stub"),
        "debug log should contain agent name, got: {}",
        logs
    );
    assert!(
        logs.contains("model"),
        "debug log should contain model info, got: {}",
        logs
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Each state transition should be logged.
/// Uses log config with file output to collect log entries.
#[test]
#[cfg(unix)]
fn cli_displays_state_transitions() {
    let tmp = std::env::temp_dir().join("tddy-cli-state-transitions");
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::create_dir_all(&tmp);

    let log_file = tmp.join("debug.log");
    let config_yaml = format!(
        r#"log:
  loggers:
    default:
      output: {{ file: "{}" }}
      format: "{{timestamp}} [{{level}}] [{{target}}] {{message}}"
  default:
    level: debug
    logger: default
"#,
        log_file.display()
    );
    let config_path = tmp.join("config.yaml");
    std::fs::write(&config_path, config_yaml).expect("write config");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "-c",
            config_path.to_str().unwrap(),
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "SKIP_QUESTIONS Build feature X",
        ])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");
    assert!(
        output.status.success(),
        "CLI should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let logs = fs::read_to_string(&log_file).unwrap_or_default();
    let has_state_info = logs.contains("Init")
        || logs.contains("Planning")
        || logs.contains("Planned")
        || logs.contains("→");
    assert!(
        has_state_info,
        "debug log should contain state transitions, got: {}",
        logs
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(unix)]
fn cli_accepts_prompt_flag_instead_of_stdin() {
    let tmp = std::env::temp_dir().join("tddy-cli-prompt-flag-test");
    let _ = std::fs::create_dir_all(&tmp);

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "SKIP_QUESTIONS Build feature from CLI arg",
        ])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected success: stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        stdout
    );
    let last_line = stdout.trim().lines().last().unwrap_or("").trim();
    let session_dir = std::path::Path::new(last_line);
    let prd = session_dir.join("artifacts").join("PRD.md");
    assert!(
        session_dir.is_dir() && prd.exists(),
        "stdout should end with plan dir path with artifacts/PRD.md: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn cli_accepts_model_flag() {
    let mut cmd = tddy_coder_bin();
    cmd.arg("--help");

    let output = cmd.output().expect("run tddy-coder --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--model") || stdout.contains("-m"),
        "help should document --model: {}",
        stdout
    );
}

#[test]
fn cli_accepts_prompt_flag_in_help() {
    let mut cmd = tddy_coder_bin();
    cmd.arg("--help");

    let output = cmd.output().expect("run tddy-coder --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--prompt"),
        "help should document --prompt: {}",
        stdout
    );
}

#[test]
fn cli_accepts_agent_flag() {
    let mut cmd = tddy_coder_bin();
    cmd.arg("--help");

    let output = cmd.output().expect("run tddy-coder --help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--agent"),
        "help should document --agent: {}",
        stdout
    );
    assert!(
        stdout.contains("claude") && stdout.contains("cursor"),
        "help should mention claude and cursor: {}",
        stdout
    );
}

#[test]
#[cfg(unix)]
fn cli_q_and_a_flow_produces_prd_after_answers() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let tmp = std::env::temp_dir().join(format!("tddy-cli-qa-{}", id));
    let _ = std::fs::create_dir_all(&tmp);

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "Build auth",
        ])
        .write_stdin("Email/password\nQ2 2025\na\n");

    let output = cmd.output().expect("run tddy-coder");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "expected success: stdout={} stderr={}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("Clarification needed") || stdout.contains("Your answer"),
        "expected Q&A prompt in stdout: {}",
        stdout
    );
    let last_line = stdout.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("");
    let session_dir = std::path::Path::new(last_line.trim());
    let prd = session_dir.join("artifacts").join("PRD.md");
    assert!(
        session_dir.is_dir() && prd.exists(),
        "stdout should end with plan dir path with artifacts/PRD.md: {}",
        stdout
    );

    let sessions_dir = tmp.join("sessions");
    let has_artifacts = sessions_dir.exists()
        && fs::read_dir(&sessions_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.path().join("artifacts").join("PRD.md").exists());
    assert!(
        has_artifacts,
        "expected artifacts/PRD.md under TDDY_SESSIONS_DIR/sessions (TODO is merged into PRD)"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(unix)]
fn cli_accepts_goal_acceptance_tests_with_session_dir() {
    let (output_dir, session_dir) = common::temp_dir_with_git_repo("at-goal-test");
    std::fs::create_dir_all(session_dir.join("artifacts")).expect("create artifacts");
    std::fs::write(
        session_dir.join("artifacts").join("PRD.md"),
        "# PRD\n## Testing Plan",
    )
    .expect("write PRD");
    common::write_changeset_for_session(&session_dir, "fake-sess", &output_dir);

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(
            TDDY_SESSIONS_DIR_ENV,
            output_dir.parent().unwrap().to_str().unwrap(),
        )
        .args([
            "--agent",
            "stub",
            "--goal",
            "acceptance-tests",
            "--session-dir",
            session_dir.to_str().unwrap(),
        ])
        .write_stdin("Yes\n");

    let output = cmd.output().expect("run tddy-coder");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected success: stderr={} stdout={}",
        stderr,
        stdout
    );
    assert!(
        stdout.contains("Created 2 tests") || stdout.contains("failing"),
        "stdout should contain test summary: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(session_dir.parent().unwrap());
}

#[test]
#[cfg(unix)]
fn cli_accepts_goal_red_with_session_dir() {
    let tmp = std::env::temp_dir().join("tddy-cli-red-goal-test");
    let _ = std::fs::create_dir_all(&tmp);

    let session_dir = tmp.join("plan-output");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    std::fs::create_dir_all(session_dir.join("artifacts")).expect("create artifacts");
    std::fs::write(
        session_dir.join("artifacts").join("PRD.md"),
        "# PRD\n## Testing Plan",
    )
    .expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- test_foo",
    )
    .expect("write acceptance-tests.md");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "red",
            "--session-dir",
            session_dir.to_str().unwrap(),
        ]);

    let output = cmd.output().expect("run tddy-coder");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected success: stderr={} stdout={}",
        stderr,
        stdout
    );
    assert!(
        stdout.contains("skeleton") || stdout.contains("test_foo"),
        "stdout should contain red output summary: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(unix)]
fn cli_accepts_goal_green_with_session_dir() {
    let tmp = std::env::temp_dir().join("tddy-cli-green-goal-test");
    let _ = std::fs::create_dir_all(&tmp);

    let session_dir = tmp.join("plan-output");
    std::fs::create_dir_all(&session_dir).expect("create plan dir");
    std::fs::create_dir_all(session_dir.join("artifacts")).expect("create artifacts");
    std::fs::write(
        session_dir.join("artifacts").join("PRD.md"),
        "# PRD\n## Testing Plan",
    )
    .expect("write PRD");
    std::fs::write(
        session_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n### test_foo\n- **File**: src/foo.rs\n- **Line**: 10\n- **Status**: failing\n",
    )
    .expect("write acceptance-tests.md");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "red",
            "--session-dir",
            session_dir.to_str().unwrap(),
        ]);

    let output = cmd.output().expect("run tddy-coder red");
    assert!(
        output.status.success(),
        "red should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut cmd2 = tddy_coder_bin();
    cmd2.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, tmp.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "green",
            "--session-dir",
            session_dir.to_str().unwrap(),
        ]);

    let output2 = cmd2.output().expect("run tddy-coder green");

    let stderr = String::from_utf8_lossy(&output2.stderr);
    let stdout = String::from_utf8_lossy(&output2.stdout);
    assert!(
        output2.status.success(),
        "expected success: stderr={} stdout={}",
        stderr,
        stdout
    );
    assert!(
        stdout.contains("passing") || stdout.contains("Implemented") || stdout.contains("impl"),
        "stdout should contain green output summary: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── Full workflow: validate + refactor after evaluate ────────────────────────

/// Full workflow (no --goal) must call validate and refactor after evaluate.
/// Uses WorkflowEngine + MockBackend to verify the chain without subprocess/sandbox issues.
#[tokio::test]
async fn full_workflow_plain_calls_validate_and_refactor_after_evaluate() {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tddy_core::changeset::read_changeset;
    use tddy_core::workflow::graph::ExecutionStatus;
    use tddy_core::{
        GoalId, MockBackend, SharedBackend, WorkflowEngine, WorkflowRecipe, WorkflowState,
    };
    use tddy_workflow_recipes::{SessionArtifactManifest, TddRecipe, TddWorkflowHooks};

    let (output_dir, session_dir) = common::temp_dir_with_git_repo("full-wf-validate-refactor");
    std::fs::create_dir_all(session_dir.join("artifacts")).expect("create artifacts");
    std::fs::write(
        session_dir.join("artifacts").join("PRD.md"),
        "# Feature PRD\n## Summary\nAuth system.",
    )
    .expect("write PRD");
    std::fs::write(session_dir.join("TODO.md"), "- [ ] Task 1").expect("write TODO");
    common::write_changeset_for_session(&session_dir, "sess-plan-1", &output_dir);

    const ACCEPTANCE_TESTS: &str = r#"{"goal":"acceptance-tests","summary":"Tests ready.","test_command":"cargo test","tests":[{"name":"t1","file":"test.rs","line":1,"status":"pass","kind":"unit"}]}"#;
    const RED: &str = r#"{"goal":"red","summary":"Failing tests written.","tests":[{"name":"t1","file":"test.rs","line":1,"status":"fail","kind":"unit"}],"skeletons":[],"markers":[],"marker_results":[]}"#;
    const GREEN: &str = r#"{"goal":"green","summary":"All tests passing.","tests":[{"name":"t1","file":"test.rs","line":1,"status":"passing"}]}"#;
    const EVALUATE: &str =
        r#"{"goal":"evaluate-changes","summary":"Changes look good.","risk_level":"low"}"#;
    const VALIDATE: &str = r#"{"goal":"validate","summary":"All subagents done.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}"#;
    const REFACTOR: &str = r#"{"goal":"refactor","summary":"Refactoring complete.","tasks_completed":3,"tests_passing":true}"#;
    const UPDATE_DOCS: &str =
        r#"{"goal":"update-docs","summary":"Documentation updated.","docs_updated":2}"#;

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS);
    backend.push_ok(RED);
    backend.push_ok(GREEN);
    backend.push_ok(EVALUATE);
    backend.push_ok(VALIDATE);
    backend.push_ok(REFACTOR);
    backend.push_ok(UPDATE_DOCS);

    let storage_dir = std::env::temp_dir().join("tddy-cli-full-wf-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let recipe: Arc<dyn WorkflowRecipe> = Arc::new(TddRecipe);
    let manifest: Arc<dyn SessionArtifactManifest> = Arc::new(TddRecipe);
    let hooks = Arc::new(TddWorkflowHooks::new(recipe.clone(), manifest));
    let engine = WorkflowEngine::new(
        recipe,
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(hooks),
    );

    let mut ctx = HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("Build auth system"),
    );
    ctx.insert(
        "session_dir".to_string(),
        serde_json::to_value(session_dir.clone()).unwrap(),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir.clone()).unwrap(),
    );
    ctx.insert("run_optional_step_x".to_string(), serde_json::json!(false));

    let result = engine
        .run_workflow_from(&GoalId::new("acceptance-tests"), ctx)
        .await
        .expect("run workflow");

    assert!(
        !matches!(result.status, ExecutionStatus::Error(_)),
        "workflow should not error: {:?}",
        result.status
    );
    assert!(
        matches!(result.status, ExecutionStatus::Completed),
        "workflow should complete: {:?}",
        result.status
    );

    assert_eq!(
        backend.invocations().len(),
        7,
        "should run acceptance-tests, red, green, evaluate, validate, refactor, update-docs"
    );

    let changeset = read_changeset(&session_dir).expect("changeset");
    assert_eq!(
        changeset.state.current,
        WorkflowState::new("DocsUpdated"),
        "state should be DocsUpdated after full workflow"
    );

    let _ = std::fs::remove_dir_all(output_dir.parent().unwrap());
}

// ── Acceptance tests for Stable session dir PRD: R1, R4 ──────────────────────

/// Running `tddy-coder --goal plan` WITHOUT `--output-dir` creates the session directory under
/// `{TDDY_SESSIONS_DIR}/sessions/{uuid}/` (or $HOME/.tddy/sessions/ when env not set) and writes
/// changeset.yaml there.
///
/// Uses TDDY_SESSIONS_DIR to a temp dir so tests do not write to production ~/.tddy.
#[test]
#[cfg(unix)]
fn test_plan_goal_cli_creates_session_under_home_tddy() {
    let tmp = std::env::temp_dir().join("tddy-cli-session-dir-home-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    // Use TDDY_SESSIONS_DIR so we do not pollute the real ~/.tddy
    let sessions_base = tmp.join("fake-sessions-base");
    std::fs::create_dir_all(&sessions_base).expect("create sessions base");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env(TDDY_SESSIONS_DIR_ENV, sessions_base.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--goal",
            "plan",
            "--prompt",
            "SKIP_QUESTIONS Build auth feature",
        ])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "plan without --output-dir should succeed and place session under TDDY_SESSIONS_DIR/sessions/; \
         stdout={} stderr={}",
        stdout,
        stderr
    );

    // TDDY_SESSIONS_DIR/sessions/ must have been created
    let sessions_dir = sessions_base.join("sessions");
    assert!(
        sessions_dir.exists(),
        "TDDY_SESSIONS_DIR/sessions/ should have been created at {}, but it does not exist",
        sessions_dir.display()
    );

    // Exactly one UUID-named subdirectory should exist inside sessions/
    let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .expect("read sessions dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "exactly one session dir should be created under {}/sessions/, got: {:?}",
        sessions_base.display(),
        entries.iter().map(|e| e.path()).collect::<Vec<_>>()
    );

    let session_dir = entries[0].path();
    let uuid_part = session_dir.file_name().unwrap().to_str().unwrap();
    assert_eq!(
        uuid_part.len(),
        36,
        "session dir name should be a 36-char UUID, got: {}",
        uuid_part
    );
    assert!(
        session_dir.join("changeset.yaml").exists(),
        "changeset.yaml should be in session dir: {}",
        session_dir.display()
    );

    assert!(
        session_dir
            .join(tddy_core::SESSION_METADATA_FILENAME)
            .exists(),
        ".session.yaml should be in session dir: {}",
        session_dir.display()
    );
    let md = tddy_core::read_session_metadata(&session_dir).expect("parse .session.yaml");
    assert_eq!(md.session_id, uuid_part);
    assert_eq!(md.status, "active");
    assert_eq!(md.tool.as_deref(), Some("tddy-coder"));
    assert!(md.repo_path.is_some());

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Bugfix plain workflow must run inside `--session-dir` when `fix-plan.md` is absent.
/// `BugfixRecipe::uses_primary_session_document` is true; `run_full_workflow_plain` must not call
/// `run_plan_to_get_dir` in that situation or it allocates a different UUID directory and the
/// explicit session folder stays empty (failed start for layouts like
/// `019d38f3-94e8-7472-bcd8-bf7cd7352248`).
#[test]
#[cfg(unix)]
fn cli_bugfix_plain_keeps_explicit_session_dir_when_fix_plan_missing() {
    let (repo_dir, _) = common::temp_dir_with_git_repo("cli-bugfix-explicit-sid");

    let sessions_root =
        std::env::temp_dir().join(format!("tddy-cli-bugfix-sessions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&sessions_root);
    std::fs::create_dir_all(&sessions_root).expect("sessions root");

    let sid = "019d38f3-94e8-7472-bcd8-bf7cd7352248";
    let session_path = sessions_root.join(SESSIONS_SUBDIR).join(sid);
    std::fs::create_dir_all(session_path.join("logs")).expect("session logs");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .current_dir(&repo_dir)
        .env(TDDY_SESSIONS_DIR_ENV, sessions_root.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--recipe",
            "bugfix",
            "--prompt",
            "SKIP_QUESTIONS repro",
            "--session-dir",
            session_path.to_str().expect("utf8 session path"),
        ]);

    let output = cmd.output().expect("run tddy-coder bugfix plain");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "bugfix plain with explicit session-dir should exit 0; stderr={}",
        stderr
    );

    assert!(
        session_path.join(".workflow").is_dir(),
        "engine storage must be created under explicit --session-dir {}, not only under another UUID",
        session_path.display()
    );

    let session_children: Vec<_> = std::fs::read_dir(sessions_root.join(SESSIONS_SUBDIR))
        .expect("read sessions/")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    assert_eq!(
        session_children.len(),
        1,
        "must not allocate a second session directory under TDDY_SESSIONS_DIR/sessions/; got {:?}",
        session_children
            .iter()
            .map(|e| e.path())
            .collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&sessions_root);
}

/// Resume merges `sessions/<id>/coder-config.yaml`. A stale `goal: plan` from a TDD-style config
/// must not prevent `--recipe bugfix` from running the full workflow (start goal `reproduce`).
#[test]
#[cfg(unix)]
fn cli_bugfix_resume_clears_stale_session_config_goal_plan() {
    let (repo_dir, _) = common::temp_dir_with_git_repo("cli-bugfix-resume-stale-goal");

    let sessions_root = std::env::temp_dir().join(format!(
        "tddy-cli-bugfix-resume-goal-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&sessions_root);
    std::fs::create_dir_all(&sessions_root).expect("sessions root");

    let sid = "019d396f-96fa-7001-8002-900300040005";
    let session_path = sessions_root.join(SESSIONS_SUBDIR).join(sid);
    std::fs::create_dir_all(session_path.join("logs")).expect("session logs");
    fs::write(
        session_path.join(tddy_coder::config::SESSION_CODER_CONFIG_FILE),
        "goal: plan\n",
    )
    .expect("write session coder-config");

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .current_dir(&repo_dir)
        .env(TDDY_SESSIONS_DIR_ENV, sessions_root.to_str().unwrap())
        .args([
            "--agent",
            "stub",
            "--recipe",
            "bugfix",
            "--prompt",
            "SKIP_QUESTIONS repro",
            "--resume-from",
            sid,
        ]);

    let output = cmd.output().expect("run tddy-coder bugfix resume");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "bugfix resume with stale goal: plan in session coder-config should still start; stderr={}",
        stderr
    );
    assert!(
        !stderr.contains("unsupported goal"),
        "should not fail single-goal routing; stderr={}",
        stderr
    );
    assert!(
        session_path.join(".workflow").is_dir(),
        "engine storage should exist under resumed session {}",
        session_path.display()
    );

    let _ = std::fs::remove_dir_all(&sessions_root);
}
