//! Acceptance tests for CLI argument parsing changes from TDD Workflow Restructure PRD.
//!
//! AC4: --goal evaluate is accepted (replaces --goal validate-changes)
//! AC12: --goal demo works standalone with --plan-dir

use assert_cmd::Command;

#[allow(deprecated)]
fn tddy_coder_bin() -> Command {
    Command::cargo_bin("tddy-coder").expect("tddy-coder binary")
}

/// AC4: CLI argument parsing accepts `--goal evaluate`.
/// `--goal validate-changes` should be rejected.
///
/// This test will fail until:
/// - The value_parser in CLI Args struct is updated to include "evaluate"
/// - "validate-changes" is removed from the value_parser
#[test]
fn cli_accepts_evaluate_goal() {
    // --goal evaluate should be recognized by the argument parser
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

    // --goal validate-changes should be rejected
    let mut cmd2 = tddy_coder_bin();
    cmd2.args(["--goal", "validate-changes"]);

    let output2 = cmd2.output().expect("run tddy-coder");
    let stderr2 = String::from_utf8_lossy(&output2.stderr);

    assert!(
        stderr2.contains("invalid value") || stderr2.contains("isn't a valid value")
            || !output2.status.success(),
        "--goal validate-changes should be rejected by the CLI parser (replaced by evaluate), stderr: {}",
        stderr2
    );
    // Specifically verify it's a parser error, not a runtime error
    assert!(
        stderr2.contains("validate-changes"),
        "error message should mention validate-changes, stderr: {}",
        stderr2
    );
}

/// AC12: `--goal demo --plan-dir <path>` is accepted by the CLI.
/// The CLI should recognize "demo" as a valid goal value.
///
/// This test will fail until:
/// - "demo" is added to the value_parser in CLI Args struct
#[test]
fn standalone_demo_goal() {
    let plan_dir = std::env::temp_dir().join("tddy-cli-demo-standalone");
    let _ = std::fs::remove_dir_all(&plan_dir);
    std::fs::create_dir_all(&plan_dir).expect("create plan dir");
    std::fs::write(
        plan_dir.join("demo-plan.md"),
        "# Demo\n## Steps\n- Run CLI\n## Verification\nOK",
    )
    .expect("write demo-plan.md");

    // --goal demo should be recognized by the argument parser
    let mut cmd = tddy_coder_bin();
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

    let _ = std::fs::remove_dir_all(&plan_dir);
}
