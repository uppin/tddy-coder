//! Shared fixtures for the PR-stack acceptance tests: in-memory `GithubPrInsightApi` and `GithubPrApi`
//! implementations plus builders for the values they serve.
//!
//! Fakes rather than mocks: the read side is stateful (a PR, its reviews, its comments, its
//! checks), and several test files need the same state seeded differently. Writes are recorded so a
//! test can assert what was *not* sent as precisely as what was.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-07-30-pr-stack-full-control.md`.
//! Changeset: `docs/dev/1-WIP/2026-07-30-pr-stack-full-control.md`.

#![allow(dead_code)] // Each integration-test binary uses a different subset of these fixtures.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use tddy_core::changeset::{
    read_changeset, write_changeset, Changeset, GithubPrStatus, Stack, StackNode,
};
use tddy_workflow_recipes::orchestrate_pr_stack::github::{
    CheckRun, GithubPrApi, GithubPrInsightApi, PrDetail, PrFile, PrIssueComment, PrLookupOutcome,
    PrRef, PrReview, PrReviewComment, PrSearchHit, PrSearchQuery, PrState, PrView,
};

pub const REPO: &str = "acme/repo";
pub const DEFAULT_BRANCH: &str = "master";

// --- stack builders ---------------------------------------------------------

/// A node that owns a branch and a child session and whose PR is recorded as open at `pull_number`.
pub fn an_open_node(node_id: &str, branch: &str, pull_number: u64, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: format!("{node_id} title"),
        description: format!("{node_id} description"),
        branch_suggestion: None,
        branch: Some(branch.to_string()),
        session_id: Some(format!("session-for-{node_id}")),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: Some(GithubPrStatus {
            phase: "open".to_string(),
            url: Some(format!("https://github.com/{REPO}/pull/{pull_number}")),
            error: None,
        }),
        child_state: None,
        internal_status: None,
    }
}

/// A node whose PR is recorded as merged.
pub fn a_merged_node(node_id: &str, branch: &str, parents: &[&str]) -> StackNode {
    StackNode {
        pr_status: Some(GithubPrStatus {
            phase: "merged".to_string(),
            url: None,
            error: None,
        }),
        ..an_open_node(node_id, branch, 0, parents)
    }
}

/// A node that was never started: a suggested branch name, no branch, no session, no PR.
pub fn a_planned_node(node_id: &str, parents: &[&str]) -> StackNode {
    StackNode {
        branch: None,
        session_id: None,
        pr_status: None,
        branch_suggestion: Some(format!("feature/stack/{node_id}")),
        ..an_open_node(node_id, "unused", 0, parents)
    }
}

pub fn write_stack(dir: &Path, nodes: Vec<StackNode>) {
    let cs = Changeset {
        stack: Some(Stack { version: 1, nodes }),
        ..Changeset::default()
    };
    write_changeset(dir, &cs).unwrap();
}

pub fn stack_of(dir: &Path) -> Stack {
    read_changeset(dir).unwrap().stack.unwrap()
}

pub fn node_ids(dir: &Path) -> Vec<String> {
    stack_of(dir)
        .nodes
        .iter()
        .map(|n| n.node_id.clone())
        .collect()
}

pub fn parents_of(dir: &Path, node_id: &str) -> Vec<String> {
    stack_of(dir).node(node_id).unwrap().parents.clone()
}

// --- rejection assertions ---------------------------------------------------

/// A rejected call, so tests read `assert_rejected(result).with_reason_containing("…")` instead of
/// unwrapping an `Err` by hand.
pub struct Rejection(String);

pub fn assert_rejected<T: std::fmt::Debug>(result: Result<T, String>) -> Rejection {
    match result {
        Err(reason) => Rejection(reason),
        Ok(value) => panic!("expected the call to be rejected, but it succeeded with {value:?}"),
    }
}

impl Rejection {
    pub fn with_reason_containing(self, fragment: &str) -> Self {
        assert!(
            self.0.contains(fragment),
            "expected the rejection to mention '{fragment}', was '{}'",
            self.0
        );
        self
    }
}

// --- PR value builders ------------------------------------------------------

/// An open PR on `head_branch`, based on `base_branch`, with everything else at a usable default.
pub fn a_pr(number: u64, head_branch: &str, base_branch: &str) -> PrDetail {
    PrDetail {
        number,
        url: format!("https://github.com/{REPO}/pull/{number}"),
        title: format!("PR {number}"),
        body: format!("body of PR {number}"),
        state: PrState::Open,
        base_branch: base_branch.to_string(),
        head_branch: head_branch.to_string(),
        head_sha: format!("sha-{number}"),
        mergeable: Some(true),
        mergeable_state: "clean".to_string(),
        additions: 10,
        deletions: 2,
        changed_files: 3,
    }
}

pub fn a_review(author: &str, state: &str, submitted_at: &str) -> PrReview {
    PrReview {
        author: author.to_string(),
        state: state.to_string(),
        body: format!("{author} says {state}"),
        submitted_at: submitted_at.to_string(),
    }
}

/// A root review comment on a diff position.
pub fn a_review_comment(id: u64, path: &str, line: u64, body: &str) -> PrReviewComment {
    PrReviewComment {
        id,
        in_reply_to_id: None,
        author: format!("author-{id}"),
        body: body.to_string(),
        path: path.to_string(),
        line: Some(line),
        diff_hunk: format!("@@ hunk for {path}"),
        created_at: format!("2026-07-30T00:00:{id:02}Z"),
    }
}

/// A reply to an existing review comment. GitHub repeats the root's anchor on every reply.
pub fn a_reply_to(id: u64, root: &PrReviewComment, body: &str) -> PrReviewComment {
    PrReviewComment {
        id,
        in_reply_to_id: Some(root.id),
        body: body.to_string(),
        ..a_review_comment(id, &root.path, root.line.unwrap_or_default(), body)
    }
}

/// The same comment, written at a stated instant rather than the one its id implies — for a test whose
/// subject is the order comments were written in.
pub fn written_at(comment: PrReviewComment, created_at: &str) -> PrReviewComment {
    PrReviewComment {
        created_at: created_at.to_string(),
        ..comment
    }
}

pub fn a_conversation_comment(author: &str, body: &str, created_at: &str) -> PrIssueComment {
    PrIssueComment {
        author: author.to_string(),
        body: body.to_string(),
        created_at: created_at.to_string(),
    }
}

pub fn a_check_run(name: &str, conclusion: &str) -> CheckRun {
    CheckRun {
        name: name.to_string(),
        conclusion: conclusion.to_string(),
    }
}

pub fn a_search_hit(number: u64, title: &str) -> PrSearchHit {
    PrSearchHit {
        number,
        title: title.to_string(),
        state: "open".to_string(),
        draft: false,
        author: "someone".to_string(),
        url: format!("https://github.com/{REPO}/pull/{number}"),
        updated_at: "2026-07-30T00:00:00Z".to_string(),
    }
}

// --- the fake ---------------------------------------------------------------

/// One recorded title/body write: the PR number, and whichever of title and body was sent.
///
/// A `None` is as meaningful as a value here — it is the assertion that a field the caller did not
/// edit was left alone on GitHub.
pub type PatchedTitleBody = (u64, Option<String>, Option<String>);

/// An in-memory `GithubPrInsightApi`. Seed it with `with_*`, then assert on `searched()` and
/// `patched_title_bodies()`.
#[derive(Default)]
pub struct FakeInsightGithub {
    prs: Mutex<Vec<PrDetail>>,
    files: Mutex<BTreeMap<u64, Vec<PrFile>>>,
    checks: Mutex<BTreeMap<String, Vec<CheckRun>>>,
    reviews: Mutex<BTreeMap<u64, Vec<PrReview>>>,
    review_comments: Mutex<BTreeMap<u64, Vec<PrReviewComment>>>,
    conversation: Mutex<BTreeMap<u64, Vec<PrIssueComment>>>,
    search_hits: Mutex<Vec<PrSearchHit>>,
    searched: Mutex<Vec<PrSearchQuery>>,
    files_requested_for: Mutex<Vec<u64>>,
    patched_title_bodies: Mutex<Vec<PatchedTitleBody>>,
}

pub fn an_insight_github() -> FakeInsightGithub {
    FakeInsightGithub::default()
}

impl FakeInsightGithub {
    pub fn with_pr(self, pr: PrDetail) -> Self {
        self.prs.lock().unwrap().push(pr);
        self
    }

    pub fn with_files(self, number: u64, files: Vec<PrFile>) -> Self {
        self.files.lock().unwrap().insert(number, files);
        self
    }

    pub fn with_check_runs(self, head_sha: &str, runs: Vec<CheckRun>) -> Self {
        self.checks
            .lock()
            .unwrap()
            .insert(head_sha.to_string(), runs);
        self
    }

    pub fn with_reviews(self, number: u64, reviews: Vec<PrReview>) -> Self {
        self.reviews.lock().unwrap().insert(number, reviews);
        self
    }

    pub fn with_review_comments(self, number: u64, comments: Vec<PrReviewComment>) -> Self {
        self.review_comments
            .lock()
            .unwrap()
            .insert(number, comments);
        self
    }

    pub fn with_conversation(self, number: u64, comments: Vec<PrIssueComment>) -> Self {
        self.conversation.lock().unwrap().insert(number, comments);
        self
    }

    pub fn with_search_hits(self, hits: Vec<PrSearchHit>) -> Self {
        *self.search_hits.lock().unwrap() = hits;
        self
    }

    /// Every search this fake was asked to run — so a test can assert the scope it was given.
    pub fn searched(&self) -> Vec<PrSearchQuery> {
        self.searched.lock().unwrap().clone()
    }

    /// Every PR whose file list was fetched — so a test can assert the fetch was skipped.
    pub fn files_requested_for(&self) -> Vec<u64> {
        self.files_requested_for.lock().unwrap().clone()
    }

    pub fn patched_title_bodies(&self) -> Vec<PatchedTitleBody> {
        self.patched_title_bodies.lock().unwrap().clone()
    }

    /// The seeded PR, or the failure every read of a pull request this fake never held produces.
    ///
    /// Every PR-scoped read goes through this, because the real implementation does too: GitHub answers
    /// an unknown number with a 404 *object*, which `json_array` rejects as "not a JSON array". A fake
    /// that served an unknown PR an empty list would let `read_pr_comments` report a PR with no
    /// feedback where production reports an error. A PR that exists with an unseeded list is a
    /// different thing, and still a legitimate empty `Ok`.
    fn pr(&self, number: u64) -> Result<PrDetail, tddy_core::WorkflowError> {
        self.prs
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.number == number)
            .cloned()
            .ok_or_else(|| {
                tddy_core::WorkflowError::WriteFailed(format!("fake holds no PR #{number}"))
            })
    }
}

impl GithubPrInsightApi for FakeInsightGithub {
    fn get_pr(&self, number: u64) -> Result<PrDetail, tddy_core::WorkflowError> {
        self.pr(number)
    }

    fn list_pr_files(&self, number: u64) -> Result<Vec<PrFile>, tddy_core::WorkflowError> {
        self.files_requested_for.lock().unwrap().push(number);
        self.pr(number)?;
        Ok(self
            .files
            .lock()
            .unwrap()
            .get(&number)
            .cloned()
            .unwrap_or_default())
    }

    /// The one list keyed by a commit rather than by a pull request: GitHub answers a commit that has
    /// no check runs with an empty, successful response, so an unseeded sha is a legitimate `Ok(vec![])`
    /// here rather than the missing-PR failure the four PR-scoped lists produce.
    fn list_check_runs(&self, head_sha: &str) -> Result<Vec<CheckRun>, tddy_core::WorkflowError> {
        Ok(self
            .checks
            .lock()
            .unwrap()
            .get(head_sha)
            .cloned()
            .unwrap_or_default())
    }

    fn list_reviews(&self, number: u64) -> Result<Vec<PrReview>, tddy_core::WorkflowError> {
        self.pr(number)?;
        Ok(self
            .reviews
            .lock()
            .unwrap()
            .get(&number)
            .cloned()
            .unwrap_or_default())
    }

    fn list_review_comments(
        &self,
        number: u64,
    ) -> Result<Vec<PrReviewComment>, tddy_core::WorkflowError> {
        self.pr(number)?;
        Ok(self
            .review_comments
            .lock()
            .unwrap()
            .get(&number)
            .cloned()
            .unwrap_or_default())
    }

    fn list_issue_comments(
        &self,
        number: u64,
    ) -> Result<Vec<PrIssueComment>, tddy_core::WorkflowError> {
        self.pr(number)?;
        Ok(self
            .conversation
            .lock()
            .unwrap()
            .get(&number)
            .cloned()
            .unwrap_or_default())
    }

    fn search_prs(
        &self,
        query: &PrSearchQuery,
    ) -> Result<Vec<PrSearchHit>, tddy_core::WorkflowError> {
        self.searched.lock().unwrap().push(query.clone());
        Ok(self.search_hits.lock().unwrap().clone())
    }

    fn patch_pr_title_body(
        &self,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<(), tddy_core::WorkflowError> {
        // Mirrors the two refusals in `RealGithubPrApi::patch_pr_title_body`: an empty payload, which
        // would spend a round trip to change nothing, and a blank title, which GitHub answers with a
        // 422. A fake that accepted either would let such a caller pass every test here and fail only
        // against api.github.com. A blank *body* is accepted, as GitHub accepts it — that is how a
        // stale description is cleared. Nothing is recorded on a refusal, because nothing would have
        // been sent.
        if title.is_none() && body.is_none() {
            return Err(tddy_core::WorkflowError::WriteFailed(format!(
                "patch_pr_title_body: neither a title nor a body was given for PR #{number}"
            )));
        }
        if title.is_some_and(|title| title.trim().is_empty()) {
            return Err(tddy_core::WorkflowError::WriteFailed(format!(
                "patch_pr_title_body: the title given for PR #{number} is blank — name a title, or \
                 leave the title out of the edit"
            )));
        }
        self.patched_title_bodies.lock().unwrap().push((
            number,
            title.map(str::to_string),
            body.map(str::to_string),
        ));
        Ok(())
    }
}

// --- the lifecycle fake -----------------------------------------------------

/// The pull number [`FakeStackGithub`] reports for whichever head branch it is asked about.
pub const A_STACK_PR: u64 = 42;

/// An in-memory [`GithubPrApi`] — the lifecycle sibling of [`FakeInsightGithub`] — that records every
/// PR lookup and every base re-target it is asked for.
///
/// What was *not* sent is as much the assertion as what was: a call that rewrites the plan and stops
/// there must be shown never to have touched GitHub, and a call that failed halfway must be shown not
/// to have re-targeted a pull request onto a base its branch does not sit on.
#[derive(Default)]
pub struct FakeStackGithub {
    looked_up: Mutex<Vec<String>>,
    patched_bases: Mutex<Vec<(u64, String)>>,
}

pub fn a_stack_github() -> FakeStackGithub {
    FakeStackGithub::default()
}

impl FakeStackGithub {
    /// Every head branch a PR was looked up for.
    pub fn looked_up(&self) -> Vec<String> {
        self.looked_up.lock().unwrap().clone()
    }

    /// Every `(pull number, new base)` this fake was asked to re-target.
    pub fn patched_bases(&self) -> Vec<(u64, String)> {
        self.patched_bases.lock().unwrap().clone()
    }
}

impl GithubPrApi for FakeStackGithub {
    fn get_open_pr(&self, head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
        self.looked_up.lock().unwrap().push(head_branch.to_string());
        Ok(Some(PrRef {
            number: A_STACK_PR,
            head_sha: format!("sha-{A_STACK_PR}"),
            base_branch: DEFAULT_BRANCH.to_string(),
            url: format!("https://github.com/{REPO}/pull/{A_STACK_PR}"),
        }))
    }

    fn get_pr_by_head(&self, _head_branch: &str) -> PrLookupOutcome {
        PrLookupOutcome::Found(PrView {
            number: A_STACK_PR,
            url: format!("https://github.com/{REPO}/pull/{A_STACK_PR}"),
            state: PrState::Open,
        })
    }

    fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
        Ok("merge-sha".to_string())
    }

    fn patch_pr_base(&self, number: u64, new_base: &str) -> Result<(), tddy_core::WorkflowError> {
        self.patched_bases
            .lock()
            .unwrap()
            .push((number, new_base.to_string()));
        Ok(())
    }

    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        Ok(99)
    }

    fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }

    fn close_pr(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Ok(())
    }
}
