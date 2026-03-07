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
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
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
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
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
    assert!(
        stdout.trim().ends_with("PRD.md"),
        "stdout should be path to PRD.md: {}",
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
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
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
    assert!(
        stdout.trim().ends_with("PRD.md"),
        "stdout should be path to PRD.md: {}",
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
    std::fs::write(plan_dir.join(".session"), "fake-sess").expect("write .session");

    let tmp_path = tmp.canonicalize().unwrap_or(tmp.clone());
    let mut cmd = tddy_coder_bin();
    cmd.env_clear()
        .env("PATH", tmp_path.to_str().unwrap())
        .env(
            "HOME",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
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
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
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
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
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
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
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
