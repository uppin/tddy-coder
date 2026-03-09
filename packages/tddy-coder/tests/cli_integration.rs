//! Integration tests for CLI argument parsing and stdin.

use assert_cmd::Command;
use std::fs;
use std::path::Path;

#[allow(deprecated)]
fn tddy_coder_bin() -> Command {
    Command::cargo_bin("tddy-coder").expect("tddy-coder binary")
}

/// Create a fake claude script that returns NDJSON with PRD+TODO in result event.
/// Uses \\n in shell so output is literal \n (JSON escape), not actual newlines.
fn create_fake_claude_prd_only(dir: &Path) -> std::io::Result<()> {
    let script = r###"#!/bin/sh
printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
printf '%s\n' '{"type":"result","subtype":"success","result":"---PRD_START---\n# Feature PRD\n## Summary\nTest feature.\n---PRD_END---\n---TODO_START---\n- [ ] Task 1\n---TODO_END---","session_id":"fake-sess","is_error":false}'
"###;
    write_executable_script(dir, "claude", script)
}

/// Create a fake claude script that returns questions (via tool_use) on first call, PRD+TODO on second.
/// Uses printf so JSON stays on one line (\\n outputs literal backslash-n for JSON).
fn create_fake_claude_script(dir: &Path) -> std::io::Result<()> {
    let script = r###"#!/bin/sh
CALL_FILE="$0.calls"
if [ -f "$CALL_FILE" ]; then
  printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
  printf '%s\n' '{"type":"result","subtype":"success","result":"---PRD_START---\n# Feature PRD\n## Summary\nUser authentication system.\n---PRD_END---\n---TODO_START---\n- [ ] Create auth module\n---TODO_END---","session_id":"fake-sess","is_error":false}'
else
  echo 1 > "$CALL_FILE"
  printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"AskUserQuestion","input":{"questions":[{"question":"What is the target audience?","header":"Audience","options":[],"multiSelect":false},{"question":"What is the expected timeline?","header":"Timeline","options":[],"multiSelect":false}]}}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","result":"","session_id":"fake-sess","is_error":false}'
fi
"###;
    write_executable_script(dir, "claude", script)
}

fn write_executable_script(dir: &Path, name: &str, script: &str) -> std::io::Result<()> {
    let script_path = dir.join(name);
    fs::write(&script_path, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms)?;
    }
    Ok(())
}

/// When --goal is omitted, the full workflow (plan -> acceptance-tests -> red -> green) runs.
/// This test fails until --goal is made optional and run_full_workflow is implemented.
#[test]
#[cfg(unix)]
fn cli_runs_full_workflow_when_goal_omitted() {
    let tmp = std::env::temp_dir().join("tddy-cli-full-workflow-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    create_fake_claude_prd_only(&tmp).expect("create fake claude");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args(["--output-dir", tmp.to_str().unwrap()])
        .write_stdin("Build auth");

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

    create_fake_claude_prd_only(&tmp).expect("create fake claude");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args(["--goal", "plan", "--output-dir", tmp.to_str().unwrap()])
        .write_stdin("Build feature X");

    let output = cmd.output().expect("run tddy-coder");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unrecognized") && !stderr.contains("unknown"),
        "should not fail on arg parsing: {}",
        stderr
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[cfg(unix)]
fn cli_accepts_output_dir_flag() {
    let tmp = std::env::temp_dir().join("tddy-cli-output-dir-test");
    let _ = std::fs::create_dir_all(&tmp);

    create_fake_claude_prd_only(&tmp).expect("create fake claude");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args(["--goal", "plan", "--output-dir", tmp.to_str().unwrap()])
        .write_stdin("Build feature Y");

    let output = cmd.output().expect("run tddy-coder");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "expected success: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan_dir = std::path::Path::new(stdout.trim());
    assert!(
        plan_dir.is_dir() && plan_dir.join("PRD.md").exists(),
        "stdout should be plan dir path with PRD.md: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Each goal should log the agent and model it is using before execution.
/// Uses --debug-output to collect log entries from the log system.
#[test]
#[cfg(unix)]
fn cli_displays_agent_and_model_before_goal_execution() {
    let tmp = std::env::temp_dir().join("tddy-cli-agent-model-display");
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::create_dir_all(&tmp);

    create_fake_claude_prd_only(&tmp).expect("create fake claude");

    let log_file = tmp.join("debug.log");
    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args([
            "--goal",
            "plan",
            "--output-dir",
            tmp.to_str().unwrap(),
            "--debug-output",
            log_file.to_str().unwrap(),
        ])
        .write_stdin("Build feature X");

    let output = cmd.output().expect("run tddy-coder");
    assert!(
        output.status.success(),
        "CLI should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let logs = fs::read_to_string(&log_file).unwrap_or_default();
    assert!(
        logs.contains("agent") && logs.contains("claude"),
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
/// Uses --debug-output to collect log entries from the log system.
#[test]
#[cfg(unix)]
fn cli_displays_state_transitions() {
    let tmp = std::env::temp_dir().join("tddy-cli-state-transitions");
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::create_dir_all(&tmp);

    create_fake_claude_prd_only(&tmp).expect("create fake claude");

    let log_file = tmp.join("debug.log");
    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args([
            "--goal",
            "plan",
            "--output-dir",
            tmp.to_str().unwrap(),
            "--debug-output",
            log_file.to_str().unwrap(),
        ])
        .write_stdin("Build feature X");

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

    create_fake_claude_prd_only(&tmp).expect("create fake claude");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args([
            "--goal",
            "plan",
            "--output-dir",
            tmp.to_str().unwrap(),
            "--prompt",
            "Build feature from CLI arg",
        ]);
    // No write_stdin — --prompt provides the description

    let output = cmd.output().expect("run tddy-coder");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected success: stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        stdout
    );
    let plan_dir = std::path::Path::new(stdout.trim());
    assert!(
        plan_dir.is_dir() && plan_dir.join("PRD.md").exists(),
        "stdout should be plan dir path with PRD.md: {}",
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

    create_fake_claude_script(&tmp).expect("create fake claude");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let path_str = tmp_path.to_str().expect("path");
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", path_str)
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args(["--goal", "plan", "--output-dir", tmp.to_str().unwrap()])
        .write_stdin("Build auth\nDevelopers\nQ2 2025\n");

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
    let last_line = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .unwrap_or("");
    let plan_dir = std::path::Path::new(last_line.trim());
    assert!(
        plan_dir.is_dir() && plan_dir.join("PRD.md").exists(),
        "stdout should end with plan dir path with PRD.md: {}",
        stdout
    );

    let has_artifacts = fs::read_dir(&tmp).unwrap().filter_map(|e| e.ok()).any(|e| {
        e.path().is_dir() && e.path().join("PRD.md").exists() && e.path().join("TODO.md").exists()
    });
    assert!(has_artifacts, "expected PRD.md and TODO.md in output dir");

    let _ = std::fs::remove_dir_all(&tmp);
}

/// Create a fake claude script that returns acceptance-tests structured output.
fn create_fake_claude_acceptance_tests(dir: &Path) -> std::io::Result<()> {
    let script = r###"#!/bin/sh
printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">{\"goal\":\"acceptance-tests\",\"summary\":\"Created 2 tests. All failing.\",\"tests\":[{\"name\":\"test_a\",\"file\":\"tests/a.rs\",\"line\":1,\"status\":\"failing\"}]}</structured-response>","session_id":"fake-sess","is_error":false}'
"###;
    write_executable_script(dir, "claude", script)
}

#[test]
#[cfg(unix)]
fn cli_accepts_goal_acceptance_tests_with_plan_dir() {
    let tmp = std::env::temp_dir().join("tddy-cli-at-goal-test");
    let _ = std::fs::create_dir_all(&tmp);

    create_fake_claude_acceptance_tests(&tmp).expect("create fake claude");

    let plan_dir = tmp.join("plan-output");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    let changeset = r#"version: 1
models: {}
sessions:
  - id: "fake-sess"
    agent: claude
    tag: plan
    created_at: "2026-03-07T10:00:00Z"
state:
  current: Planned
  updated_at: "2026-03-07T10:00:00Z"
  history: []
artifacts: {}
"#;
    std::fs::write(plan_dir.join("changeset.yaml"), changeset).expect("write changeset");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args([
            "--goal",
            "acceptance-tests",
            "--plan-dir",
            plan_dir.to_str().unwrap(),
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
        stdout.contains("Created 2 tests") || stdout.contains("failing"),
        "stdout should contain test summary: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn cli_errors_when_plan_dir_missing_for_evaluate_goal() {
    let mut cmd = tddy_coder_bin();
    cmd.args(["--goal", "evaluate"]);

    let output = cmd.output().expect("run tddy-coder");

    assert!(
        !output.status.success(),
        "should fail when --plan-dir missing for evaluate goal"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plan-dir") || stderr.contains("plan_dir"),
        "error should mention plan-dir: {}",
        stderr
    );
}

#[test]
fn cli_errors_when_plan_dir_missing_for_acceptance_tests_goal() {
    let mut cmd = tddy_coder_bin();
    cmd.args(["--goal", "acceptance-tests"]);
    // --plan-dir is NOT provided

    let output = cmd.output().expect("run tddy-coder");

    assert!(
        !output.status.success(),
        "should fail when --plan-dir missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plan-dir") || stderr.contains("plan_dir"),
        "error should mention plan-dir: {}",
        stderr
    );
}

/// Create a fake claude script that returns red goal structured output.
fn create_fake_claude_red(dir: &Path) -> std::io::Result<()> {
    let script = r###"#!/bin/sh
printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">{\"goal\":\"red\",\"summary\":\"Created 1 skeleton and 1 failing test.\",\"tests\":[{\"name\":\"test_foo\",\"file\":\"src/foo.rs\",\"line\":10,\"status\":\"failing\"}],\"skeletons\":[{\"name\":\"Foo\",\"file\":\"src/foo.rs\",\"line\":5,\"kind\":\"struct\"}]}</structured-response>","session_id":"fake-sess","is_error":false}'
"###;
    write_executable_script(dir, "claude", script)
}

#[test]
#[cfg(unix)]
fn cli_accepts_goal_red_with_plan_dir() {
    let tmp = std::env::temp_dir().join("tddy-cli-red-goal-test");
    let _ = std::fs::create_dir_all(&tmp);

    create_fake_claude_red(&tmp).expect("create fake claude");

    let plan_dir = tmp.join("plan-output");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        plan_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n- test_foo",
    )
    .expect("write acceptance-tests.md");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args(["--goal", "red", "--plan-dir", plan_dir.to_str().unwrap()]);

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
fn cli_errors_when_plan_dir_missing_for_red_goal() {
    let mut cmd = tddy_coder_bin();
    cmd.args(["--goal", "red"]);
    // --plan-dir is NOT provided

    let output = cmd.output().expect("run tddy-coder");

    assert!(
        !output.status.success(),
        "should fail when --plan-dir missing for red goal"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plan-dir") || stderr.contains("plan_dir"),
        "error should mention plan-dir: {}",
        stderr
    );
}

/// Create a fake claude script that returns red on first call, green on second.
fn create_fake_claude_red_then_green(dir: &Path) -> std::io::Result<()> {
    let script = r###"#!/bin/sh
CALL_FILE="$0.calls"
if [ -f "$CALL_FILE" ]; then
  printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
  printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">{\"goal\":\"green\",\"summary\":\"Implemented. All tests passing.\",\"tests\":[{\"name\":\"test_foo\",\"file\":\"src/foo.rs\",\"line\":10,\"status\":\"passing\"}],\"implementations\":[{\"name\":\"Foo\",\"file\":\"src/foo.rs\",\"line\":5,\"kind\":\"struct\"}]}</structured-response>","session_id":"fake-sess","is_error":false}'
else
  echo 1 > "$CALL_FILE"
  printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
  printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">{\"goal\":\"red\",\"summary\":\"Created 1 skeleton and 1 failing test.\",\"tests\":[{\"name\":\"test_foo\",\"file\":\"src/foo.rs\",\"line\":10,\"status\":\"failing\"}],\"skeletons\":[{\"name\":\"Foo\",\"file\":\"src/foo.rs\",\"line\":5,\"kind\":\"struct\"}]}</structured-response>","session_id":"fake-sess","is_error":false}'
fi
"###;
    write_executable_script(dir, "claude", script)
}

#[test]
#[cfg(unix)]
fn cli_accepts_goal_green_with_plan_dir() {
    let tmp = std::env::temp_dir().join("tddy-cli-green-goal-test");
    let _ = std::fs::create_dir_all(&tmp);

    create_fake_claude_red_then_green(&tmp).expect("create fake claude");

    let plan_dir = tmp.join("plan-output");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(plan_dir.join("PRD.md"), "# PRD\n## Testing Plan").expect("write PRD");
    std::fs::write(
        plan_dir.join("acceptance-tests.md"),
        "# Acceptance Tests\n## Tests\n### test_foo\n- **File**: src/foo.rs\n- **Line**: 10\n- **Status**: failing\n",
    )
    .expect("write acceptance-tests.md");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args(["--goal", "red", "--plan-dir", plan_dir.to_str().unwrap()]);

    let output = cmd.output().expect("run tddy-coder red");
    assert!(
        output.status.success(),
        "red should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut cmd2 = tddy_coder_bin();
    cmd2.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args(["--goal", "green", "--plan-dir", plan_dir.to_str().unwrap()]);

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

#[test]
fn cli_errors_when_plan_dir_missing_for_green_goal() {
    let mut cmd = tddy_coder_bin();
    cmd.args(["--goal", "green"]);
    // --plan-dir is NOT provided

    let output = cmd.output().expect("run tddy-coder");

    assert!(
        !output.status.success(),
        "should fail when --plan-dir missing for green goal"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plan-dir") || stderr.contains("plan_dir"),
        "error should mention plan-dir: {}",
        stderr
    );
}

// ── Full workflow: validate + refactor after evaluate ────────────────────────

/// Create a fake claude script that handles all 7 workflow goals (no demo).
/// Determines the goal from the prompt content (`-p` argument) and returns
/// the matching structured response.
/// On the validate call, writes refactoring-plan.md to the working directory.
fn create_fake_claude_full_workflow(dir: &Path) -> std::io::Result<()> {
    let script = r###"#!/bin/sh
PROMPT=""
while [ $# -gt 0 ]; do
  case "$1" in
    -p) PROMPT="$2"; shift 2 ;;
    *) shift ;;
  esac
done

printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'

case "$PROMPT" in
  *"Create a PRD"*)
    printf '%s\n' '{"type":"result","subtype":"success","result":"---PRD_START---\n# Feature PRD\n## Summary\nAuth system.\n---PRD_END---\n---TODO_START---\n- [ ] Task 1\n---TODO_END---","session_id":"s","is_error":false}'
    ;;
  *"Create acceptance tests based on"*)
    printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">\n{\"goal\":\"acceptance-tests\",\"summary\":\"Tests ready.\",\"test_command\":\"cargo test\",\"tests\":[{\"name\":\"t1\",\"file\":\"test.rs\",\"line\":1,\"status\":\"pass\",\"kind\":\"unit\"}]}\n</structured-response>","session_id":"s","is_error":false}'
    ;;
  *"Create skeleton code and failing"*)
    printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">\n{\"goal\":\"red\",\"summary\":\"Failing tests written.\",\"tests\":[{\"name\":\"t1\",\"file\":\"test.rs\",\"line\":1,\"status\":\"fail\",\"kind\":\"unit\"}],\"skeletons\":[],\"markers\":[],\"marker_results\":[]}\n</structured-response>","session_id":"s","is_error":false}'
    ;;
  *"make all failing tests pass"*)
    printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">\n{\"goal\":\"green\",\"summary\":\"All tests passing.\",\"tests\":[{\"name\":\"t1\",\"file\":\"test.rs\",\"line\":1,\"status\":\"passing\"}]}\n</structured-response>","session_id":"s","is_error":false}'
    ;;
  *"Analyze the current git changes"*)
    printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">\n{\"goal\":\"evaluate-changes\",\"summary\":\"Changes look good.\",\"risk_level\":\"low\"}\n</structured-response>","session_id":"s","is_error":false}'
    ;;
  *"Orchestrate a full refactor validation"*)
    printf '# Refactoring Plan\n## Tasks\n1. Extract shared helper\n' > "$PWD/refactoring-plan.md"
    printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">\n{\"goal\":\"validate\",\"summary\":\"All subagents done.\",\"tests_report_written\":true,\"prod_ready_report_written\":true,\"clean_code_report_written\":true,\"refactoring_plan_written\":true}\n</structured-response>","session_id":"s","is_error":false}'
    ;;
  *"Execute the refactoring tasks"*)
    printf '%s\n' '{"type":"result","subtype":"success","result":"<structured-response content-type=\"application-json\">\n{\"goal\":\"refactor\",\"summary\":\"Refactoring complete.\",\"tasks_completed\":3,\"tests_passing\":true}\n</structured-response>","session_id":"s","is_error":false}'
    ;;
  *)
    printf '%s\n' '{"type":"result","subtype":"success","result":"---PRD_START---\n# Fallback PRD\n## Summary\nFallback.\n---PRD_END---\n---TODO_START---\n- [ ] Fallback\n---TODO_END---","session_id":"s","is_error":false}'
    ;;
esac
"###;
    write_executable_script(dir, "claude", script)
}

/// Full workflow (no --goal) must call validate and refactor after evaluate.
/// Currently the workflow stops after evaluate — this test verifies all 7 goals run.
#[test]
#[cfg(unix)]
fn full_workflow_plain_calls_validate_and_refactor_after_evaluate() {
    let tmp = std::env::temp_dir().join("tddy-cli-full-wf-validate-refactor");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    create_fake_claude_full_workflow(&tmp).expect("create fake claude");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());

    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME")
                .unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().into_owned()),
        )
        .args([
            "--output-dir",
            tmp.to_str().unwrap(),
            "--prompt",
            "Build auth system",
        ]);

    let output = cmd.output().expect("run tddy-coder");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "full workflow should succeed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    assert!(
        stdout.contains("Refactoring complete") || stdout.contains("tasks_completed"),
        "stdout should contain refactor output (validate+refactor ran after evaluate).\nstdout: {}",
        stdout
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
