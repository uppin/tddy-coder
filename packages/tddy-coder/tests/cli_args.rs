//! Acceptance tests for CLI argument parsing changes from TDD Workflow Restructure PRD.
//!
//! AC4: --goal evaluate is accepted (replaces --goal validate-changes)
//! AC12: --goal demo works standalone with --plan-dir

use assert_cmd::Command;
use std::fs;
use std::path::Path;

#[allow(deprecated)]
fn tddy_coder_bin() -> Command {
    Command::cargo_bin("tddy-coder").expect("tddy-coder binary")
}

/// Fake claude script that exits immediately with minimal NDJSON.
/// Prevents tests from blocking on real agent invocation.
#[cfg(unix)]
fn create_fake_claude_quick_exit(dir: &Path) -> std::io::Result<()> {
    let script = r#"#!/bin/sh
printf '%s\n' '{"type":"system","subtype":"init","session_id":"fake-sess"}'
printf '%s\n' '{"type":"result","subtype":"success","result":"","session_id":"fake-sess","is_error":false}'
exit 0
"#;
    let script_path = dir.join("claude");
    fs::write(&script_path, script)?;
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&script_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms)?;
    Ok(())
}

/// AC4: CLI argument parsing accepts `--goal evaluate`.
#[test]
fn cli_accepts_evaluate_goal() {
    let mut cmd = tddy_coder_bin();
    cmd.args(["--goal", "evaluate"]);

    let output = cmd.output().expect("run tddy-coder");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The command may fail for other reasons (e.g. missing --plan-dir),
    // but it should NOT fail because "evaluate" is an invalid goal value.
    assert!(
        !stderr.contains("invalid value 'evaluate'")
            && !stderr.contains("'evaluate' isn't a valid value"),
        "--goal evaluate should be accepted by the CLI parser, stderr: {}",
        stderr
    );
}

/// AC12: `--goal demo --plan-dir <path>` is accepted by the CLI.
/// The CLI should recognize "demo" as a valid goal value.
///
/// This test will fail until:
/// - "demo" is added to the value_parser in CLI Args struct
#[test]
#[cfg(unix)]
fn standalone_demo_goal() {
    let tmp = std::env::temp_dir().join("tddy-cli-demo-standalone");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    create_fake_claude_quick_exit(&tmp).expect("create fake claude");

    let plan_dir = tmp.join("plan");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    let path = format!(
        "{}:{}",
        tmp.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut cmd = tddy_coder_bin();
    cmd.env("PATH", &path);
    cmd.args(["--goal", "demo", "--plan-dir", plan_dir.to_str().unwrap()]);

    let output = cmd.output().expect("run tddy-coder");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should NOT fail because "demo" is an invalid goal value.
    assert!(
        !stderr.contains("invalid value 'demo'") && !stderr.contains("'demo' isn't a valid value"),
        "--goal demo should be accepted by the CLI parser, stderr: {}",
        stderr
    );

    // Without --plan-dir, demo should error (requires plan dir)
    let mut cmd2 = tddy_coder_bin();
    cmd2.env("PATH", &path);
    cmd2.args(["--goal", "demo"]);

    let output2 = cmd2.output().expect("run tddy-coder");

    assert!(
        !output2.status.success(),
        "--goal demo without --plan-dir should fail (demo requires plan-dir)"
    );

    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    assert!(
        stderr2.contains("plan-dir") || stderr2.contains("plan_dir") || stderr2.contains("demo"),
        "error should mention plan-dir or demo requirement, stderr: {}",
        stderr2
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// AC1: `--goal validate` is accepted by CLI; `--goal validate-refactor` is rejected.
///
/// This test will fail until:
/// - "validate" is added to the value_parser in CLI Args struct
/// - "validate-refactor" is removed from the value_parser
#[test]
fn cli_accepts_validate_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-cli-validate-test");
    // --goal validate should be recognized by the argument parser
    let mut cmd = tddy_coder_bin();
    cmd.args([
        "--goal",
        "validate",
        "--plan-dir",
        plan_dir.to_str().expect("temp path"),
    ]);

    let output = cmd.output().expect("run tddy-coder");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("invalid value 'validate'")
            && !stderr.contains("'validate' isn't a valid value"),
        "--goal validate should be accepted by the CLI parser, stderr: {}",
        stderr
    );

    // --goal validate-refactor should be rejected
    let mut cmd2 = tddy_coder_bin();
    cmd2.args(["--goal", "validate-refactor"]);

    let output2 = cmd2.output().expect("run tddy-coder");
    let stderr2 = String::from_utf8_lossy(&output2.stderr);

    assert!(
        stderr2.contains("invalid value") || stderr2.contains("isn't a valid value"),
        "--goal validate-refactor should be rejected by the CLI parser, stderr: {}",
        stderr2
    );
}

/// `--goal update-docs --plan-dir <path>` is accepted by CLI.
#[test]
#[cfg(unix)]
fn cli_accepts_update_docs_goal() {
    let tmp = std::env::temp_dir().join("tddy-cli-update-docs-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    create_fake_claude_quick_exit(&tmp).expect("create fake claude");

    let plan_dir = tmp.join("plan");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");

    let path = format!(
        "{}:{}",
        tmp.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut cmd = tddy_coder_bin();
    cmd.env("PATH", &path);
    cmd.args([
        "--goal",
        "update-docs",
        "--plan-dir",
        plan_dir.to_str().unwrap(),
    ]);

    let output = cmd.output().expect("run tddy-coder");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("invalid value 'update-docs'")
            && !stderr.contains("'update-docs' isn't a valid value"),
        "--goal update-docs should be accepted by the CLI parser, stderr: {}",
        stderr
    );

    // --goal update-docz should be rejected (typo)
    let mut cmd2 = tddy_coder_bin();
    cmd2.env("PATH", &path);
    cmd2.args(["--goal", "update-docz"]);

    let output2 = cmd2.output().expect("run tddy-coder");
    let stderr2 = String::from_utf8_lossy(&output2.stderr);

    assert!(
        stderr2.contains("invalid value") || stderr2.contains("isn't a valid value"),
        "--goal update-docz should be rejected by the CLI parser, stderr: {}",
        stderr2
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// AC2: `--goal refactor --plan-dir <path>` is accepted by CLI.
///
/// This test will fail until:
/// - "refactor" is added to the value_parser in CLI Args struct
#[test]
#[cfg(unix)]
fn cli_accepts_refactor_goal() {
    let tmp = std::env::temp_dir().join("tddy-cli-refactor-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    create_fake_claude_quick_exit(&tmp).expect("create fake claude");

    let plan_dir = tmp.join("plan");
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("refactoring-plan.md"),
        "# Refactoring Plan\n## Tasks\n- Rename method\n",
    )
    .expect("write refactoring-plan.md");

    let path = format!(
        "{}:{}",
        tmp.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut cmd = tddy_coder_bin();
    cmd.env("PATH", &path);
    cmd.args([
        "--goal",
        "refactor",
        "--plan-dir",
        plan_dir.to_str().unwrap(),
    ]);

    let output = cmd.output().expect("run tddy-coder");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("invalid value 'refactor'")
            && !stderr.contains("'refactor' isn't a valid value"),
        "--goal refactor should be accepted by the CLI parser, stderr: {}",
        stderr
    );

    // Without --plan-dir, refactor should error (requires plan dir)
    let mut cmd2 = tddy_coder_bin();
    cmd2.env("PATH", &path);
    cmd2.args(["--goal", "refactor"]);

    let output2 = cmd2.output().expect("run tddy-coder");

    assert!(
        !output2.status.success(),
        "--goal refactor without --plan-dir should fail (refactor requires plan-dir)"
    );

    let stderr2 = String::from_utf8_lossy(&output2.stderr);
    assert!(
        stderr2.contains("plan-dir")
            || stderr2.contains("plan_dir")
            || stderr2.contains("refactor"),
        "error should mention plan-dir or refactor requirement, stderr: {}",
        stderr2
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

/// --debug-output creates file and redirects debug logs to it (no stderr corruption).
#[test]
#[cfg(unix)]
fn debug_output_redirects_logs_to_file() {
    let tmp = std::env::temp_dir().join("tddy-debug-output-test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    let debug_file = tmp.join("debug.log");

    let script = r#"#!/bin/sh
printf '%s\n' '{"type":"system","session_id":"t1"}'
printf '%s\n' '{"type":"result","result":"","session_id":"t1"}'
exit 0
"#;
    let script_path = tmp.join("cursor");
    std::fs::write(&script_path, script).expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();
    }

    let mut cmd = tddy_coder_bin();
    let path = format!(
        "{}:{}",
        tmp.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    cmd.env("PATH", path);
    cmd.args([
        "--goal",
        "plan",
        "--prompt",
        "test feature",
        "--output-dir",
        tmp.to_str().unwrap(),
        "--debug-output",
        debug_file.to_str().unwrap(),
        "--agent",
        "cursor",
    ]);

    let _output = cmd.output().expect("run tddy-coder");

    assert!(debug_file.exists(), "debug output file should be created");
    let content = std::fs::read_to_string(&debug_file).expect("read debug file");
    assert!(
        content.contains("[DEBUG]"),
        "debug output file should contain debug-level log entries, got: {}",
        content
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
