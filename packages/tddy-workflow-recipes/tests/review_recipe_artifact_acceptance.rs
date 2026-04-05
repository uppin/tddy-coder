//! PRD acceptance: `review.md` persisted under the session directory after successful branch review workflow.

use std::path::PathBuf;

use tddy_workflow_recipes::review::persist_review_md_to_session_dir;

/// Successful **review** workflow must leave `session_dir/review.md` (basename) with substantive body text.
#[test]
fn review_recipe_persists_review_md() {
    let session_dir: PathBuf =
        std::env::temp_dir().join(format!("tddy-review-md-contract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&session_dir);
    std::fs::create_dir_all(&session_dir).expect("session dir");

    let json = serde_json::json!({
        "goal": "branch-review",
        "summary": "Branch review complete with substantive markdown body for the session artifact.",
        "validity_assessment": "ok",
        "review_body_markdown": "# Branch review\n\n## Findings\n- Observed issue in module X.\n\nAdditional detail to satisfy minimum length for acceptance."
    })
    .to_string();

    persist_review_md_to_session_dir(&session_dir, &json).expect("persist review.md from submit");

    let path = session_dir.join("review.md");
    assert!(
        path.is_file(),
        "review workflow must persist review.md under session_dir (expected {}); wire ReviewRecipe submit → review.md",
        path.display()
    );
    let body = std::fs::read_to_string(&path).expect("read review.md");
    assert!(
        body.len() >= 40,
        "review.md must be non-empty substantive text (len={})",
        body.len()
    );
    assert!(
        body.contains("# Branch review") || body.contains("## Findings"),
        "review.md must include a stable heading or marker substring; got: {}",
        body
    );

    let _ = std::fs::remove_dir_all(&session_dir);
}
