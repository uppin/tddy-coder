//! `review.md` persistence from `branch-review` submit (library + CLI).

use assert_cmd::cargo::cargo_bin_cmd;
use std::path::PathBuf;

#[test]
fn persist_review_md_from_submit_accepts_minimal_valid_json() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let json = serde_json::json!({
        "goal": "branch-review",
        "summary": "Done.",
        "validity_assessment": "ok",
        "review_body_markdown": "# Branch review\n\n## Findings\n- Note"
    })
    .to_string();
    // When
    let r = tddy_workflow_recipes::review::persist_review_md_to_session_dir(dir.path(), &json);
    // Then
    assert!(r.is_ok(), "must write review.md and return Ok; got {:?}", r);
    let path: PathBuf = dir.path().join("review.md");
    assert!(path.is_file(), "expected {}", path.display());
}

/// The CLI used to write `review.md` itself when a `branch-review` submission found no socket but a
/// `TDDY_SESSION_DIR` — a local run that reported success without reaching any session. A
/// `tddy-tools` with no session to report to is an error now (`#agent-tools 15/19`), so nothing is
/// persisted and the agent is told where a session could have come from.
#[test]
fn submit_branch_review_cli_without_a_session_fails_and_writes_nothing() {
    // Given
    let dir = tempfile::tempdir().expect("tempdir");
    let nowhere = tempfile::tempdir().expect("a directory with no pointer above it");
    let json = serde_json::json!({
        "goal": "branch-review",
        "summary": "Done.",
        "validity_assessment": "ok",
        "review_body_markdown": "# Branch review\n\n## Findings\n- Note"
    })
    .to_string();

    // When
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET")
        .env("TDDY_SESSION_DIR", dir.path())
        .current_dir(nowhere.path())
        .args(["submit", "--goal", "branch-review", "--data", &json]);
    let output = cmd.output().expect("run tddy-tools submit");

    // Then
    assert_eq!(
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).contains(
                "pass --socket <path>, run from a directory at or below a .tddy/session.json"
            ),
            dir.path().join("review.md").exists(),
        ),
        (Some(1), true, false)
    );
}
