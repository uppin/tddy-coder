//! Persist **`review.md`** from validated branch-review submit JSON.

use std::fs;
use std::path::Path;

use super::parse::parse_branch_review_output;
use super::REVIEW_MD_BASENAME;

/// Write `session_dir/review.md` from a **`branch-review`** JSON payload (same shape as `tddy-tools submit`).
pub fn persist_review_md_to_session_dir(session_dir: &Path, json: &str) -> Result<(), String> {
    log::info!(
        "persist_review_md_to_session_dir: session_dir={}",
        session_dir.display()
    );
    let out = parse_branch_review_output(json)?;
    fs::create_dir_all(session_dir).map_err(|e| e.to_string())?;
    let path = session_dir.join(REVIEW_MD_BASENAME);
    fs::write(
        &path,
        out.review_body_markdown.trim_end_matches('\n').to_string() + "\n",
    )
    .map_err(|e| format!("write {}: {e}", path.display()))?;
    log::info!(
        "persist_review_md_to_session_dir: wrote {} bytes to {}",
        out.review_body_markdown.len(),
        path.display()
    );
    Ok(())
}
