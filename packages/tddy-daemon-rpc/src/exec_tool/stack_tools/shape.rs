//! The agent-facing JSON of the stack read tools: pull requests, their feedback and search hits.
//!
//! Owned here rather than by a `Serialize` derive on the views, so the wire shape is a deliberate
//! contract instead of whatever field list the Rust types happen to carry. Moved from `tddy-tools`
//! with the tools themselves (`#orchestrators` 7/11); the shapes are unchanged.

use serde_json::{json, Value};
use tddy_github::pr_api::{PrSearchHit, PrState};
use tddy_pr_stack::pr_insight;

/// Shape a [`pr_insight::PrReadView`] for the wire.
///
/// Owned by the tool rather than by a `Serialize` derive on the view, so the agent-facing shape is a
/// deliberate contract instead of whatever field list the Rust type happens to carry. `files` is
/// absent (not `null`) when the caller did not ask for it, matching "neither fetched nor returned".
pub(super) fn pr_read_json(view: &pr_insight::PrReadView) -> Value {
    let mut out = json!({
        "number": view.number,
        "url": view.url,
        "title": view.title,
        "body": view.body,
        "state": pr_state_name(view.state),
        "base": view.base_branch,
        "head": view.head_branch,
        "head_sha": view.head_sha,
        "mergeable": view.mergeable,
        "mergeable_state": view.mergeable_state,
        "additions": view.additions,
        "deletions": view.deletions,
        "changed_files": view.changed_files,
        "reviews": view.reviews.iter().map(|r| json!({
            "author": r.author,
            "state": r.state,
        })).collect::<Vec<_>>(),
        "checks": view.checks.iter().map(|c| json!({
            "name": c.name,
            "conclusion": c.conclusion,
        })).collect::<Vec<_>>(),
    });
    if let Some(files) = &view.files {
        out["files"] = files
            .iter()
            .map(|f| json!({ "path": f.path, "status": f.status }))
            .collect::<Vec<_>>()
            .into();
    }
    out
}

/// A PR's live state in the lowercase vocabulary the tool surface already uses for a node's phase,
/// keeping `draft` distinct from `open` — the two differ to a reviewer.
fn pr_state_name(state: PrState) -> &'static str {
    match state {
        PrState::Open => "open",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
        PrState::Draft => "draft",
    }
}

/// Shape a [`pr_insight::PrCommentsView`] for the wire.
///
/// The three sections stay
/// separate keys because a verdict, a diff-anchored conversation and a PR-wide comment answer
/// different questions.
pub(super) fn pr_comments_json(view: &pr_insight::PrCommentsView) -> Value {
    json!({
        "reviews": view.reviews.iter().map(|r| json!({
            "author": r.author,
            "state": r.state,
            "body": r.body,
            "submitted_at": r.submitted_at,
        })).collect::<Vec<_>>(),
        // No `resolved` on a thread: resolution state exists only on GitHub's GraphQL
        // `reviewThreads`, and emitting a guessed value would mislead the agent.
        "threads": view.threads.iter().map(|t| json!({
            "path": t.path,
            "line": t.line,
            "diff_hunk": t.diff_hunk,
            "comments": t.comments.iter().map(|c| json!({
                "author": c.author,
                "body": c.body,
                "created_at": c.created_at,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "conversation": view.conversation.iter().map(|c| json!({
            "author": c.author,
            "body": c.body,
            "created_at": c.created_at,
        })).collect::<Vec<_>>(),
    })
}

/// Shape a page of search hits for the wire.
///
/// No branch fields —
/// GitHub's search reports neither head nor base, so a hit that needs them is followed up with
/// `pr_read`.
pub(super) fn pr_search_json(hits: &[PrSearchHit]) -> Value {
    json!({
        "hits": hits.iter().map(|h| json!({
            "number": h.number,
            "title": h.title,
            "state": h.state,
            "draft": h.draft,
            "author": h.author,
            "url": h.url,
            "updated_at": h.updated_at,
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use tddy_github::pr_api::{CheckRun, PrFile, PrIssueComment, PrReview};

    use super::*;

    /// A read view with one review, one check run and no file list — the shape `pr_read` returns when
    /// the caller did not ask for files.
    fn a_pr_read_view() -> pr_insight::PrReadView {
        pr_insight::PrReadView {
            number: 42,
            url: "https://github.com/acme/repo/pull/42".to_string(),
            title: "Add the token store".to_string(),
            body: "Extracted from the parent PR.".to_string(),
            state: PrState::Open,
            base_branch: "master".to_string(),
            head_branch: "feature/auth/token-store".to_string(),
            head_sha: "sha-42".to_string(),
            mergeable: Some(true),
            mergeable_state: "clean".to_string(),
            additions: 10,
            deletions: 2,
            changed_files: 3,
            reviews: vec![pr_insight::ReviewerState {
                author: "alice".to_string(),
                state: "APPROVED".to_string(),
            }],
            checks: vec![CheckRun {
                name: "build".to_string(),
                conclusion: "success".to_string(),
            }],
            files: None,
        }
    }

    #[test]
    fn a_pr_read_without_files_leaves_the_files_key_out_altogether() {
        // Given — the caller did not ask for the file list, so none was fetched
        let view = a_pr_read_view();

        // When
        let json = pr_read_json(&view);

        // Then — absent, not `null`: "neither fetched nor returned" and "fetched and empty" are
        // different answers, and only an absent key says the first
        assert_eq!(json.get("files"), None);
    }

    #[test]
    fn a_pr_read_puts_the_prs_branches_on_the_wire_as_base_and_head() {
        // Given — a PR whose file list the caller did ask for
        let view = pr_insight::PrReadView {
            files: Some(vec![PrFile {
                path: "src/token_store.rs".to_string(),
                status: "added".to_string(),
            }]),
            ..a_pr_read_view()
        };

        // When
        let json = pr_read_json(&view);

        // Then — the whole agent-facing contract, `base`/`head` included: the wire names are shorter
        // than the Rust field names on purpose, and a rename would be a breaking change to the prompt
        assert_eq!(
            json,
            json!({
                "number": 42,
                "url": "https://github.com/acme/repo/pull/42",
                "title": "Add the token store",
                "body": "Extracted from the parent PR.",
                "state": "open",
                "base": "master",
                "head": "feature/auth/token-store",
                "head_sha": "sha-42",
                "mergeable": true,
                "mergeable_state": "clean",
                "additions": 10,
                "deletions": 2,
                "changed_files": 3,
                "reviews": [{ "author": "alice", "state": "APPROVED" }],
                "checks": [{ "name": "build", "conclusion": "success" }],
                "files": [{ "path": "src/token_store.rs", "status": "added" }],
            })
        );
    }

    #[test]
    fn a_prs_live_state_reaches_the_agent_in_the_lowercase_phase_vocabulary() {
        // Given / When — each state GitHub can report
        let names = [
            PrState::Open,
            PrState::Draft,
            PrState::Merged,
            PrState::Closed,
        ]
        .map(pr_state_name);

        // Then — `draft` stays distinct from `open`: the two differ to a reviewer
        assert_eq!(names, ["open", "draft", "merged", "closed"]);
    }

    #[test]
    fn pr_comments_puts_reviews_threads_and_conversation_on_the_wire_as_three_sections() {
        // Given — one verdict, one two-comment thread anchored to a diff line, one PR-wide comment
        let view = pr_insight::PrCommentsView {
            reviews: vec![PrReview {
                author: "alice".to_string(),
                state: "CHANGES_REQUESTED".to_string(),
                body: "Name the timeout.".to_string(),
                submitted_at: "2026-07-30T10:00:00Z".to_string(),
            }],
            threads: vec![pr_insight::PrReviewThread {
                path: "src/token_store.rs".to_string(),
                line: Some(17),
                diff_hunk: "@@ -1,3 +1,4 @@".to_string(),
                comments: vec![
                    pr_insight::PrThreadComment {
                        author: "alice".to_string(),
                        body: "Why 30?".to_string(),
                        created_at: "2026-07-30T10:01:00Z".to_string(),
                    },
                    pr_insight::PrThreadComment {
                        author: "bob".to_string(),
                        body: "The provider's own p99.".to_string(),
                        created_at: "2026-07-30T10:02:00Z".to_string(),
                    },
                ],
            }],
            conversation: vec![PrIssueComment {
                author: "carol".to_string(),
                body: "Rebased onto master.".to_string(),
                created_at: "2026-07-30T10:03:00Z".to_string(),
            }],
        };

        // When
        let json = pr_comments_json(&view);

        // Then — the whole agent-facing contract, and no `resolved` on the thread: resolution state
        // exists only on GraphQL's `reviewThreads`, so a guessed value would mislead the agent
        assert_eq!(
            json,
            json!({
                "reviews": [{
                    "author": "alice",
                    "state": "CHANGES_REQUESTED",
                    "body": "Name the timeout.",
                    "submitted_at": "2026-07-30T10:00:00Z",
                }],
                "threads": [{
                    "path": "src/token_store.rs",
                    "line": 17,
                    "diff_hunk": "@@ -1,3 +1,4 @@",
                    "comments": [
                        { "author": "alice", "body": "Why 30?", "created_at": "2026-07-30T10:01:00Z" },
                        { "author": "bob", "body": "The provider's own p99.", "created_at": "2026-07-30T10:02:00Z" },
                    ],
                }],
                "conversation": [{
                    "author": "carol",
                    "body": "Rebased onto master.",
                    "created_at": "2026-07-30T10:03:00Z",
                }],
            })
        );
        assert_eq!(json["threads"][0].get("resolved"), None);
    }

    #[test]
    fn a_pr_search_puts_each_hit_on_the_wire_without_the_branches_github_does_not_report() {
        // Given — one open PR and one draft, as a search page returns them
        let hits = vec![
            PrSearchHit {
                number: 42,
                title: "Add the token store".to_string(),
                state: "open".to_string(),
                draft: false,
                author: "alice".to_string(),
                url: "https://github.com/acme/repo/pull/42".to_string(),
                updated_at: "2026-07-30T10:00:00Z".to_string(),
            },
            PrSearchHit {
                number: 43,
                title: "Rotate the signing key".to_string(),
                state: "open".to_string(),
                draft: true,
                author: "bob".to_string(),
                url: "https://github.com/acme/repo/pull/43".to_string(),
                updated_at: "2026-07-30T11:00:00Z".to_string(),
            },
        ];

        // When
        let json = pr_search_json(&hits);

        // Then — the whole agent-facing contract, in the order the search returned it; no `base` or
        // `head`, which GitHub's search does not report — `pr_read` is where those come from
        assert_eq!(
            json,
            json!({
                "hits": [
                    {
                        "number": 42,
                        "title": "Add the token store",
                        "state": "open",
                        "draft": false,
                        "author": "alice",
                        "url": "https://github.com/acme/repo/pull/42",
                        "updated_at": "2026-07-30T10:00:00Z",
                    },
                    {
                        "number": 43,
                        "title": "Rotate the signing key",
                        "state": "open",
                        "draft": true,
                        "author": "bob",
                        "url": "https://github.com/acme/repo/pull/43",
                        "updated_at": "2026-07-30T11:00:00Z",
                    },
                ],
            })
        );
    }
}
