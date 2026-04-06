//! Parse structured **`branch-review`** submit JSON.

use serde::Deserialize;

use super::TASK_BRANCH_REVIEW;

/// Parsed **`branch-review`** payload (aligned with `branch-review.schema.json`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchReviewOutput {
    pub goal: String,
    pub summary: String,
    pub validity_assessment: String,
    pub review_body_markdown: String,
}

/// Validate and parse JSON for the branch-review goal.
pub fn parse_branch_review_output(json: &str) -> Result<BranchReviewOutput, String> {
    let v: BranchReviewOutput =
        serde_json::from_str(json).map_err(|e| format!("branch-review JSON: {e}"))?;
    if v.goal != TASK_BRANCH_REVIEW {
        return Err(format!(
            "branch-review JSON: goal must be {:?}, got {:?}",
            TASK_BRANCH_REVIEW, v.goal
        ));
    }
    if v.review_body_markdown.trim().is_empty() {
        return Err("branch-review JSON: review_body_markdown must be non-empty".into());
    }
    Ok(v)
}
