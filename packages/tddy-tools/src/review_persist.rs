//! Persist `review.md` from **`branch-review`** structured submit (invoked by presenter / agent tooling).

use std::path::Path;

use tddy_workflow_recipes::review::persist_review_md_to_session_dir;

/// Apply validated `branch-review` JSON and write `session_dir/review.md`.
pub fn persist_review_md_from_branch_review_json(
    session_dir: &Path,
    json: &str,
) -> Result<(), String> {
    log::info!(
        target: "tddy_tools::review_persist",
        "persist_review_md_from_branch_review_json session_dir={}",
        session_dir.display()
    );
    persist_review_md_to_session_dir(session_dir, json)
}
