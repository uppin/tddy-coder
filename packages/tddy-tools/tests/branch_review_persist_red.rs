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
    let r =
        tddy_tools::review_persist::persist_review_md_from_branch_review_json(dir.path(), &json);
    // Then
    assert!(r.is_ok(), "must write review.md and return Ok; got {:?}", r);
    let path: PathBuf = dir.path().join("review.md");
    assert!(path.is_file(), "expected {}", path.display());
}

#[test]
fn submit_branch_review_cli_writes_review_md_when_session_dir_set() {
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
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET")
        .env("TDDY_SESSION_DIR", dir.path())
        .args(["submit", "--goal", "branch-review", "--data", &json]);
    let assert = cmd.assert().success();

    // Then
    let out = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        out.contains("\"status\":\"ok\""),
        "expected ok JSON on stdout; got {out}"
    );
    assert!(
        out.contains("branch-review"),
        "expected 'branch-review' in stdout; got {out}"
    );

    let path = dir.path().join("review.md");
    assert!(
        path.is_file(),
        "CLI must persist review.md under TDDY_SESSION_DIR; missing {}",
        path.display()
    );
}
