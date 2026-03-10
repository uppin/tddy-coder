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
        .args([
            "--output-dir",
            tmp.to_str().unwrap(),
            "--prompt",
            "Build auth",
        ])
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
            "Build feature X",
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
            "Build feature for approval test",
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
    let plan_dir = std::path::Path::new(last_line);
    assert!(
        plan_dir.is_dir() && plan_dir.join("PRD.md").exists(),
        "stdout should end with plan dir path with PRD.md, last_line={} stdout={}",
        last_line,
        stdout
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
        .args([
            "--goal",
            "plan",
            "--output-dir",
            tmp.to_str().unwrap(),
            "--prompt",
            "Build feature Y",
        ])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "expected success: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let last_line = stdout.trim().lines().last().unwrap_or("").trim();
    let plan_dir = std::path::Path::new(last_line);
    assert!(
        plan_dir.is_dir() && plan_dir.join("PRD.md").exists(),
        "stdout should end with plan dir path with PRD.md: {}",
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
            "--prompt",
            "Build feature X",
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
            "--prompt",
            "Build feature X",
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
    let plan_dir = std::path::Path::new(last_line);
    assert!(
        plan_dir.is_dir() && plan_dir.join("PRD.md").exists(),
        "stdout should end with plan dir path with PRD.md: {}",
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
        .args([
            "--goal",
            "plan",
            "--output-dir",
            tmp.to_str().unwrap(),
            "--prompt",
            "Build auth",
        ])
        .write_stdin("Developers\nQ2 2025\na\n");

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
        e.path().is_dir() && e.path().join("PRD.md").exists()
    });
    assert!(has_artifacts, "expected PRD.md in output dir (TODO is merged into PRD)");

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

/// Full workflow (no --goal) must call validate and refactor after evaluate.
/// Uses WorkflowEngine + MockBackend to verify the chain without subprocess/sandbox issues.
#[tokio::test]
async fn full_workflow_plain_calls_validate_and_refactor_after_evaluate() {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tddy_core::changeset::read_changeset;
    use tddy_core::output::slugify_directory_name;
    use tddy_core::workflow::graph::ExecutionStatus;
    use tddy_core::workflow::tdd_hooks::TddWorkflowHooks;
    use tddy_core::{MockBackend, SharedBackend, WorkflowEngine};

    let output_dir = std::env::temp_dir().join("tddy-cli-full-wf-validate-refactor");
    let _ = std::fs::remove_dir_all(&output_dir);
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    let plan_dir = output_dir.join(slugify_directory_name("Build auth system"));
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("PRD.md"),
        "# Feature PRD\n## Summary\nAuth system.",
    )
    .expect("write PRD");
    std::fs::write(plan_dir.join("TODO.md"), "- [ ] Task 1").expect("write TODO");
    let changeset = r#"version: 1
models: {}
sessions:
  - id: "sess-plan-1"
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

    const ACCEPTANCE_TESTS: &str = r#"<structured-response content-type="application-json">
{"goal":"acceptance-tests","summary":"Tests ready.","test_command":"cargo test","tests":[{"name":"t1","file":"test.rs","line":1,"status":"pass","kind":"unit"}]}
</structured-response>"#;
    const RED: &str = r#"<structured-response content-type="application-json">
{"goal":"red","summary":"Failing tests written.","tests":[{"name":"t1","file":"test.rs","line":1,"status":"fail","kind":"unit"}],"skeletons":[],"markers":[],"marker_results":[]}
</structured-response>"#;
    const GREEN: &str = r#"<structured-response content-type="application-json">
{"goal":"green","summary":"All tests passing.","tests":[{"name":"t1","file":"test.rs","line":1,"status":"passing"}]}
</structured-response>"#;
    const EVALUATE: &str = r#"<structured-response content-type="application-json">
{"goal":"evaluate-changes","summary":"Changes look good.","risk_level":"low"}
</structured-response>"#;
    const VALIDATE: &str = r#"<structured-response content-type="application-json">
{"goal":"validate","summary":"All subagents done.","tests_report_written":true,"prod_ready_report_written":true,"clean_code_report_written":true,"refactoring_plan_written":true}
</structured-response>"#;
    const REFACTOR: &str = r#"<structured-response content-type="application-json">
{"goal":"refactor","summary":"Refactoring complete.","tasks_completed":3,"tests_passing":true}
</structured-response>"#;

    let backend = Arc::new(MockBackend::new());
    backend.push_ok(ACCEPTANCE_TESTS);
    backend.push_ok(RED);
    backend.push_ok(GREEN);
    backend.push_ok(EVALUATE);
    backend.push_ok(VALIDATE);
    backend.push_ok(REFACTOR);

    let storage_dir = std::env::temp_dir().join("tddy-cli-full-wf-engine");
    let _ = std::fs::remove_dir_all(&storage_dir);
    let engine = WorkflowEngine::new(
        SharedBackend::from_arc(backend.clone()),
        storage_dir,
        Some(Arc::new(TddWorkflowHooks::new())),
    );

    let mut ctx = HashMap::new();
    ctx.insert(
        "feature_input".to_string(),
        serde_json::json!("Build auth system"),
    );
    ctx.insert(
        "plan_dir".to_string(),
        serde_json::to_value(plan_dir.clone()).unwrap(),
    );
    ctx.insert(
        "output_dir".to_string(),
        serde_json::to_value(output_dir.clone()).unwrap(),
    );
    ctx.insert("run_demo".to_string(), serde_json::json!(false));

    let result = engine
        .run_workflow_from("acceptance-tests", ctx)
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
        6,
        "should run acceptance-tests, red, green, evaluate, validate, refactor"
    );

    let changeset = read_changeset(&plan_dir).expect("changeset");
    assert_eq!(
        changeset.state.current, "RefactorComplete",
        "state should be RefactorComplete after full workflow"
    );

    let _ = std::fs::remove_dir_all(&output_dir);
}

// ── Acceptance tests for Stable session dir PRD: R1, R4 ──────────────────────

/// Running `tddy-coder --goal plan` WITHOUT `--output-dir` creates the session directory under
/// `$HOME/.tddy/sessions/{uuid}/` and writes changeset.yaml there.
///
/// Fails until the plan goal generates a session dir from $HOME/.tddy instead of requiring
/// --output-dir and creating a YYYY-MM-DD-slug subdirectory.
#[test]
#[cfg(unix)]
fn test_plan_goal_cli_creates_session_under_home_tddy() {
    let tmp = std::env::temp_dir().join("tddy-cli-session-dir-home-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    create_fake_claude_prd_only(&tmp).expect("create fake claude");

    // Use a controlled fake HOME so we do not pollute the real ~/.tddy
    let fake_home = tmp.join("fake-home");
    std::fs::create_dir_all(&fake_home).expect("create fake home");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env("HOME", fake_home.to_str().unwrap())
        .args(["--goal", "plan", "--prompt", "Build auth feature"])
        .write_stdin("a\n");

    let output = cmd.output().expect("run tddy-coder");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "plan without --output-dir should succeed and place session under $HOME/.tddy/sessions/; \
         stdout={} stderr={}",
        stdout,
        stderr
    );

    // $HOME/.tddy/sessions/ must have been created
    let sessions_dir = fake_home.join(".tddy").join("sessions");
    assert!(
        sessions_dir.exists(),
        "$HOME/.tddy/sessions/ should have been created at {}, but it does not exist",
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
        fake_home.join(".tddy").display(),
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

    let _ = std::fs::remove_dir_all(&tmp);
}
