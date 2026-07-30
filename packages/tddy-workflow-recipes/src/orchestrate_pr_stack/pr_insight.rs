//! Read-side shaping for the orchestrator's PR-inspection tools.
//!
//! Every function here takes `&dyn GithubPrInsightApi`, so the shape of what the agent sees is
//! testable against a fake rather than only against `api.github.com`. The MCP tool bodies in
//! `tddy-tools` resolve environment and serialize — they hold no shaping logic, the same split
//! `add_planned_pr_node` / `pr_add_planned` already established.
//!
//! PRD: `docs/ft/coder/pr-stacking.md` § Full control over the plan.
//! Changeset: `docs/dev/changesets.md` (2026-07-30, pr-stack-full-control).

use std::collections::BTreeMap;

use tddy_core::changeset::Stack;

use super::github::{
    CheckRun, GithubPrInsightApi, PrFile, PrIssueComment, PrReview, PrReviewComment, PrSearchHit,
    PrSearchQuery, PrState,
};

/// Hits a single search page may return: GitHub's own maximum `per_page`, and the PRD's hard cap.
const MAX_SEARCH_LIMIT: u32 = 100;

/// Hits a caller gets when it expresses no preference.
const DEFAULT_SEARCH_LIMIT: u32 = 20;

/// The latest review state a single reviewer left.
///
/// A reviewer can submit many reviews on one PR; only the last one is their standing position, so
/// the earlier ones are folded away rather than reported as separate rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerState {
    pub author: String,
    pub state: String,
}

/// One review-comment thread: a root comment plus its replies, in the order they were written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrReviewThread {
    pub path: String,
    pub line: Option<u64>,
    pub diff_hunk: String,
    pub comments: Vec<PrThreadComment>,
}

/// One comment inside a thread. The anchor (`path`, `line`, `diff_hunk`) lives on the thread, so it
/// is not repeated per comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrThreadComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// One pull request as `pr_read` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrReadView {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub body: String,
    pub state: PrState,
    pub base_branch: String,
    pub head_branch: String,
    pub head_sha: String,
    pub mergeable: Option<bool>,
    pub mergeable_state: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
    pub reviews: Vec<ReviewerState>,
    pub checks: Vec<CheckRun>,
    /// `None` unless the caller asked for the file list — a large PR's files would otherwise
    /// dominate the agent's context.
    pub files: Option<Vec<PrFile>>,
}

/// A PR's review feedback, split by kind.
///
/// Carries no thread-resolution state: `isResolved` exists only on GraphQL's `reviewThreads`, and
/// emitting a guessed value would be worse than omitting it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrCommentsView {
    pub reviews: Vec<PrReview>,
    pub threads: Vec<PrReviewThread>,
    pub conversation: Vec<PrIssueComment>,
}

/// What the agent may choose about a search. The repository is not among these — it is supplied by
/// the caller from the orchestrator's own remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSearchInput {
    pub text: Option<String>,
    pub state: String,
    pub author: Option<String>,
    pub base: Option<String>,
    pub limit: u32,
}

/// Read one PR in full: its own fields, one review state per reviewer, and its head commit's check
/// runs. The file list is fetched only when `include_files` asks for it.
///
/// Check runs are passed through in the order GitHub reported them, an in-progress run keeping its
/// empty `conclusion` — an agent deciding whether a PR is mergeable needs "not finished yet" to look
/// different from "finished without success".
pub fn read_pr(
    gh: &dyn GithubPrInsightApi,
    number: u64,
    include_files: bool,
) -> Result<PrReadView, tddy_core::WorkflowError> {
    let pr = gh.get_pr(number)?;
    let reviews = latest_state_per_reviewer(gh.list_reviews(number)?);
    let checks = gh.list_check_runs(&pr.head_sha)?;
    // Not fetched at all unless asked for: a large PR's file list would dominate the agent's
    // context, and a request whose result is then discarded is a request worth not making.
    let files = if include_files {
        Some(gh.list_pr_files(number)?)
    } else {
        None
    };

    Ok(PrReadView {
        number: pr.number,
        url: pr.url,
        title: pr.title,
        body: pr.body,
        state: pr.state,
        base_branch: pr.base_branch,
        head_branch: pr.head_branch,
        head_sha: pr.head_sha,
        mergeable: pr.mergeable,
        mergeable_state: pr.mergeable_state,
        additions: pr.additions,
        deletions: pr.deletions,
        changed_files: pr.changed_files,
        reviews,
        checks,
        files,
    })
}

/// Fold a PR's whole review history into each reviewer's standing position.
///
/// A reviewer who commented, then requested changes, then approved has one position — the last one.
/// Keyed by author in a `BTreeMap` so the result is ordered by author rather than by whatever order
/// GitHub happened to return.
fn latest_state_per_reviewer(reviews: Vec<PrReview>) -> Vec<ReviewerState> {
    let mut latest: BTreeMap<String, PrReview> = BTreeMap::new();
    for review in reviews {
        match latest.get(&review.author) {
            Some(seen) if seen.submitted_at > review.submitted_at => {}
            _ => {
                latest.insert(review.author.clone(), review);
            }
        }
    }
    latest
        .into_values()
        .map(|review| ReviewerState {
            author: review.author,
            state: review.state,
        })
        .collect()
}

/// Read a PR's review feedback: submitted reviews, diff-anchored comments grouped into threads, and
/// conversation comments.
///
/// The three sections stay separate because they answer different questions: a review is a verdict, a
/// thread is a conversation about one diff position, and a conversation comment belongs to the PR as
/// a whole. Merging them would lose which is which.
pub fn read_pr_comments(
    gh: &dyn GithubPrInsightApi,
    number: u64,
) -> Result<PrCommentsView, tddy_core::WorkflowError> {
    Ok(PrCommentsView {
        reviews: gh.list_reviews(number)?,
        threads: threads_of(gh.list_review_comments(number)?),
        conversation: gh.list_issue_comments(number)?,
    })
}

/// Rebuild threads from the flat review-comment list GitHub returns.
///
/// A thread is a root comment (`in_reply_to_id == None`) plus every comment whose reply chain reaches
/// it, so a reply to a reply lands in the root's thread rather than starting one of its own. Threads
/// come out ordered by root id and each thread's comments by `created_at`.
///
/// A reply whose chain leaves the list — the parent lives on a page that was not fetched — becomes
/// its own thread rather than being dropped: showing feedback under a slightly wrong root is
/// recoverable, silently hiding it is not.
fn threads_of(comments: Vec<PrReviewComment>) -> Vec<PrReviewThread> {
    let by_id: BTreeMap<u64, &PrReviewComment> = comments.iter().map(|c| (c.id, c)).collect();

    let mut grouped: BTreeMap<u64, Vec<&PrReviewComment>> = BTreeMap::new();
    for comment in &comments {
        grouped
            .entry(root_id_of(comment, &by_id))
            .or_default()
            .push(comment);
    }

    grouped
        .into_iter()
        .filter_map(|(root_id, mut thread)| {
            thread.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            let root = by_id.get(&root_id)?;
            Some(PrReviewThread {
                path: root.path.clone(),
                line: root.line,
                diff_hunk: root.diff_hunk.clone(),
                comments: thread
                    .into_iter()
                    .map(|c| PrThreadComment {
                        author: c.author.clone(),
                        body: c.body.clone(),
                        created_at: c.created_at.clone(),
                    })
                    .collect(),
            })
        })
        .collect()
}

/// Walk a comment's `in_reply_to_id` chain to the root it belongs under.
///
/// Bounded by the number of comments so a malformed chain (a parent pointer that loops) terminates
/// instead of hanging the agent's tool call.
fn root_id_of(comment: &PrReviewComment, by_id: &BTreeMap<u64, &PrReviewComment>) -> u64 {
    let mut current = comment;
    for _ in 0..by_id.len() {
        let Some(parent_id) = current.in_reply_to_id else {
            break;
        };
        let Some(parent) = by_id.get(&parent_id) else {
            break;
        };
        current = parent;
    }
    current.id
}

/// Search `repo`'s pull requests, returning at most `input.limit` hits.
///
/// `repo` is the caller's, never the agent's: a search is a read of *this* repository's PRs. The
/// limit is enforced on the way out as well as requested on the way in, so a server that ignores
/// `per_page` cannot hand the agent more than it asked for.
pub fn search_repository_prs(
    gh: &dyn GithubPrInsightApi,
    repo: &str,
    input: PrSearchInput,
) -> Result<Vec<PrSearchHit>, tddy_core::WorkflowError> {
    let limit = effective_search_limit(input.limit);
    let query = PrSearchQuery {
        repo: repo.to_string(),
        text: input.text,
        state: input.state,
        author: input.author,
        base: input.base,
        limit,
    };
    let mut hits = gh.search_prs(&query)?;
    hits.truncate(limit as usize);
    Ok(hits)
}

/// A usable page size: `0` means "no preference", and anything above one page is capped, since
/// search is deliberately un-paginated.
fn effective_search_limit(requested: u32) -> u32 {
    match requested {
        0 => DEFAULT_SEARCH_LIMIT,
        n => n.min(MAX_SEARCH_LIMIT),
    }
}

/// The pull number a stack node refers to, recovered from the URL recorded in its `pr_status`.
///
/// This is the system's existing "which PR is this node" mechanism (see
/// `bridge::pr_number_from_status_url`); a node that records no PR URL is not addressable by
/// `node_id` and says so rather than guessing a number.
pub fn pull_number_for_node(stack: &Stack, node_id: &str) -> Result<u64, String> {
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("pull_number_for_node: node '{node_id}' not found"))?;
    super::bridge::pr_number_from_status_url(node.pr_status.as_ref()).ok_or_else(|| {
        format!(
            "pull_number_for_node: node '{node_id}' records no pull request url, so it cannot be \
             addressed by node id — name the pull number instead"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_search_that_states_no_limit_returns_the_default_twenty_hits() {
        // Given — `0` is how a caller says it has no preference
        let requested = 0;

        // When
        let limit = effective_search_limit(requested);

        // Then
        assert_eq!(limit, 20);
    }

    #[test]
    fn a_limit_larger_than_one_page_is_capped_at_a_hundred_hits() {
        // Given — more hits than a single un-paginated search can return
        let requested = 250;

        // When
        let limit = effective_search_limit(requested);

        // Then
        assert_eq!(limit, 100);
    }
}
