//! The PR resource family as GitHub's REST JSON reports it, parsed for the daemon's GitHub inbox
//! (`#workqueue` 2/14). Kept apart from `pr_api.rs`, which reads the same shapes inside its
//! blocking `curl` methods and is over its size budget.

use serde_json::Value;

use crate::pr_api::{pr_state_from_github, PrDetail, PrIssueComment, PrReview, PrReviewComment};

/// A string field, or `""` when GitHub reports it as `null` (an empty PR body, a review with no
/// text). Distinguishing "absent" from "empty" would not change any caller's decision.
fn text(item: &Value, field: &str) -> String {
    item.get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn pointer_text(item: &Value, path: &str) -> String {
    item.pointer(path)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// The author login; `""` for an item whose author's account is gone (GitHub reports `null`).
fn author(item: &Value) -> String {
    pointer_text(item, "/user/login")
}

/// GitHub's `id` of an item: every review and comment has one, and an item without it could not be
/// addressed (replied to, reacted to, told apart from its neighbours) — so its absence is an error.
fn required_id(item: &Value, what: &str) -> Result<u64, String> {
    item.get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{what} has no id: {item}"))
}

fn array(body: &str) -> Result<Vec<Value>, String> {
    match serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))? {
        Value::Array(items) => Ok(items),
        other => Err(format!("expected a JSON array, got: {other}")),
    }
}

/// One pull request object (`GET pulls/{n}`, or an item of `GET pulls`).
pub fn parse_pr_detail(pr: &Value) -> Result<PrDetail, String> {
    let number = pr
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("a pull request has no number: {pr}"))?;
    // No default: `state` decides what `work_pr_read` reports and what phase an adopted node is created
    // at, so reading a missing field as "open" would present a merged or closed PR as in play.
    let state = pr
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("pull request #{number} has no state"))?;
    let merged_at = pr.get("merged_at").and_then(Value::as_str);
    let draft = pr.get("draft").and_then(Value::as_bool).unwrap_or(false);
    let count = |field: &str| pr.get(field).and_then(Value::as_u64).unwrap_or(0);
    Ok(PrDetail {
        number,
        url: text(pr, "html_url"),
        title: text(pr, "title"),
        body: text(pr, "body"),
        state: pr_state_from_github(state, merged_at, draft),
        base_branch: pointer_text(pr, "/base/ref"),
        head_branch: pointer_text(pr, "/head/ref"),
        head_sha: pointer_text(pr, "/head/sha"),
        // `null` while GitHub computes mergeability: kept as `None`, never read as conflicted.
        mergeable: pr.get("mergeable").and_then(Value::as_bool),
        mergeable_state: text(pr, "mergeable_state"),
        additions: count("additions"),
        deletions: count("deletions"),
        changed_files: count("changed_files"),
    })
}

/// A `GET pulls/{n}` body.
pub fn parse_pr_body(body: &str) -> Result<PrDetail, String> {
    let pr: Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    parse_pr_detail(&pr)
}

/// A `GET pulls?head=…` body: the first pull request, if any.
pub fn parse_first_pr(body: &str) -> Result<Option<PrDetail>, String> {
    array(body)?.first().map(parse_pr_detail).transpose()
}

/// A `GET pulls/{n}/reviews` body.
pub fn parse_reviews(body: &str) -> Result<Vec<PrReview>, String> {
    array(body)?
        .iter()
        .map(|review| {
            Ok(PrReview {
                id: required_id(review, "review")?,
                author: author(review),
                state: text(review, "state"),
                body: text(review, "body"),
                submitted_at: text(review, "submitted_at"),
            })
        })
        .collect()
}

/// A `GET pulls/{n}/comments` body.
pub fn parse_review_comments(body: &str) -> Result<Vec<PrReviewComment>, String> {
    array(body)?
        .iter()
        .map(|comment| {
            Ok(PrReviewComment {
                // Without an id a comment cannot be placed in a thread and no reply could attach to it.
                id: required_id(comment, "review comment")?,
                in_reply_to_id: comment.get("in_reply_to_id").and_then(Value::as_u64),
                author: author(comment),
                body: text(comment, "body"),
                path: text(comment, "path"),
                // `null` once the diff position it was anchored to has gone stale.
                line: comment.get("line").and_then(Value::as_u64),
                diff_hunk: text(comment, "diff_hunk"),
                created_at: text(comment, "created_at"),
            })
        })
        .collect()
}

/// A `GET issues/{n}/comments` body.
pub fn parse_issue_comments(body: &str) -> Result<Vec<PrIssueComment>, String> {
    array(body)?
        .iter()
        .map(|comment| {
            Ok(PrIssueComment {
                id: required_id(comment, "conversation comment")?,
                author: author(comment),
                body: text(comment, "body"),
                created_at: text(comment, "created_at"),
            })
        })
        .collect()
}
