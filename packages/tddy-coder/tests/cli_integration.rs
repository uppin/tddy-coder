//! Integration tests for CLI argument parsing and stdin.

use assert_cmd::Command;
use std::fs;
use std::path::Path;


#[allow(deprecated)]
fn tddy_coder_bin() -> Command {
    Command::cargo_bin("tddy-coder").expect("tddy-coder binary")
}

/// Create a fake claude script that returns PRD+TODO immediately (for single-call tests).
fn create_fake_claude_prd_only(dir: &Path) -> std::io::Result<()> {
    let script = r###"#!/bin/sh
echo "---PRD_START---"
echo "# Feature PRD"
echo "## Summary"
echo "Test feature."
echo "---PRD_END---"
echo "---TODO_START---"
echo "- [ ] Task 1"
echo "---TODO_END---"
"###;
    write_executable_script(dir, "claude", script)
}

/// Create a fake claude script that returns QUESTIONS on first call, PRD+TODO on second.
fn create_fake_claude_script(dir: &Path) -> std::io::Result<()> {
    // Use file existence to track call count - avoids needing 'cat' in PATH.
    // First call: create marker file and output QUESTIONS. Second call: output PRD+TODO.
    let script = r###"#!/bin/sh
CALL_FILE="$0.calls"
if [ -f "$CALL_FILE" ]; then
  echo "---PRD_START---"
  echo "# Feature PRD"
  echo "## Summary"
  echo "User authentication system."
  echo "---PRD_END---"
  echo "---TODO_START---"
  echo "- [ ] Create auth module"
  echo "---TODO_END---"
else
  echo 1 > "$CALL_FILE"
  echo "---QUESTIONS_START---"
  echo "What is the target audience?"
  echo "What is the expected timeline?"
  echo "---QUESTIONS_END---"
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
        .env("HOME", std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
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
        .env("HOME", std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
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
        stdout.contains("Output:") || stdout.contains("Planning complete"),
        "stdout: {}",
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
        .env("HOME", std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
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
    assert!(stdout.contains("Planning complete"));

    let has_artifacts = fs::read_dir(&tmp)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path().is_dir()
                && e.path().join("PRD.md").exists()
                && e.path().join("TODO.md").exists()
        });
    assert!(has_artifacts, "expected PRD.md and TODO.md in output dir");

    let _ = std::fs::remove_dir_all(&tmp);
}
