//! GitHub REST PR API abstraction for orchestrate-pr-stack.

/// Resolved open PR reference.
#[derive(Debug, Clone)]
pub struct PrRef {
    pub number: u64,
    pub head_sha: String,
    pub base_branch: String,
    pub url: String,
}

/// Live GitHub state of a PR, surfaced on the PR-Stack Chat Screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
    Draft,
}

/// A PR (open, merged, or closed) resolved by head branch, with its derived state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrView {
    pub number: u64,
    pub url: String,
    pub state: PrState,
}

/// Outcome of resolving a PR by head branch.
///
/// The distinction between [`Self::NotFound`] and [`Self::Unavailable`] is the point: a lookup that
/// could not be performed (no credential, rate limit, transport error) must never read as "this
/// branch has no PR" — that silent conflation is why a live open PR stayed invisible on the
/// PR-Stack screen. Because every outcome is a value, a lookup can never fail the caller's RPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrLookupOutcome {
    /// A PR (open, merged, or closed) exists for the head branch.
    Found(PrView),
    /// GitHub was queried and reported no PR for the head branch.
    NotFound,
    /// The lookup could not be performed; carries an operator-facing reason.
    Unavailable(String),
}

/// Qualify a head branch as GitHub's `head` filter requires: `owner:branch`.
///
/// `GET /repos/{owner}/{repo}/pulls?head=…` **ignores** an unqualified value rather than rejecting
/// it, returning the repository's whole PR list — so an unqualified head makes `arr.first()` an
/// arbitrary, unrelated PR. `repo` is `owner/name`; only the owner segment prefixes the head.
///
/// An already-qualified head is returned unchanged (qualifying twice matches nothing), and a `repo`
/// with no owner segment yields the bare branch — inventing an owner would query another repository.
#[must_use]
pub fn qualified_head(repo: &str, branch: &str) -> String {
    if branch.contains(':') {
        return branch.to_string();
    }
    match repo.split_once('/') {
        Some((owner, _)) if !owner.is_empty() => format!("{owner}:{branch}"),
        _ => branch.to_string(),
    }
}

/// Derive the displayed `PrState` from GitHub's PR fields.
///
/// A closed PR with a `merged_at` timestamp is merged; a closed PR without one was closed
/// unmerged. An open PR marked `draft` is a draft; otherwise it is open.
pub fn pr_state_from_github(state: &str, merged_at: Option<&str>, draft: bool) -> PrState {
    match state {
        "closed" if merged_at.is_some() => PrState::Merged,
        "closed" => PrState::Closed,
        _ if draft => PrState::Draft,
        _ => PrState::Open,
    }
}

/// Abstraction over GitHub REST PR operations. Allows stubbing in tests.
pub trait GithubPrApi: Send + Sync {
    /// Find open PR whose head matches `head_branch` (format: `owner:branch`).
    fn get_open_pr(&self, head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError>;

    /// Find the PR (open, merged, or closed) whose head matches `head_branch`, with its derived
    /// state. `head_branch` may be a bare branch — implementations qualify it (see
    /// [`qualified_head`]).
    ///
    /// Every outcome is a value, including "could not look up" ([`PrLookupOutcome::Unavailable`]),
    /// so a display-path caller can degrade the PR field without failing its own call.
    fn get_pr_by_head(&self, head_branch: &str) -> PrLookupOutcome;

    /// Merge PR by number; returns the merge commit SHA.
    fn merge_pr(&self, number: u64) -> Result<String, tddy_core::WorkflowError>;

    /// PATCH the base branch of an open PR.
    fn patch_pr_base(&self, number: u64, new_base: &str) -> Result<(), tddy_core::WorkflowError>;

    /// Create a new PR; returns the PR number.
    fn create_pr(
        &self,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<u64, tddy_core::WorkflowError>;

    /// Disable auto-merge on a PR (e.g. after repoint to avoid premature merge).
    fn disable_auto_merge(&self, number: u64) -> Result<(), tddy_core::WorkflowError>;

    /// Close a PR without merging (PATCH `{"state":"closed"}`).
    fn close_pr(&self, number: u64) -> Result<(), tddy_core::WorkflowError>;
}

/// One pull request in full, as `GET /repos/{repo}/pulls/{number}` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDetail {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub body: String,
    pub state: PrState,
    pub base_branch: String,
    pub head_branch: String,
    pub head_sha: String,
    /// GitHub reports `null` while it is still computing mergeability.
    pub mergeable: Option<bool>,
    pub mergeable_state: String,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
}

/// One file a PR touches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrFile {
    pub path: String,
    /// GitHub's own vocabulary: `added` / `modified` / `removed` / `renamed` / …
    pub status: String,
}

/// One check run against a PR's head commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRun {
    pub name: String,
    /// `success` / `failure` / `neutral` / `cancelled` / `timed_out` / `action_required`, or
    /// empty while the run is still in progress.
    pub conclusion: String,
}

/// One submitted review on a PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrReview {
    pub author: String,
    /// `APPROVED` / `CHANGES_REQUESTED` / `COMMENTED` / `DISMISSED`.
    pub state: String,
    pub body: String,
    pub submitted_at: String,
}

/// One review comment anchored to a diff position.
///
/// `id` and `in_reply_to_id` are what make thread reconstruction possible: the REST API returns a
/// flat list, and a thread is a root comment plus every comment whose `in_reply_to_id` chains back
/// to it. There is deliberately no `resolved` field — thread resolution is exposed only by the
/// GraphQL API, and inventing the value here would be a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrReviewComment {
    pub id: u64,
    pub in_reply_to_id: Option<u64>,
    pub author: String,
    pub body: String,
    pub path: String,
    /// `None` for a comment on an outdated diff position.
    pub line: Option<u64>,
    pub diff_hunk: String,
    pub created_at: String,
}

/// One conversation (issue-level) comment on a PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrIssueComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// A PR search, always scoped to one repository.
///
/// `repo` is set by the caller from the orchestrator's own remote, never by the agent — a search is
/// a read of *this* repository's PRs, not a way to reach another one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSearchQuery {
    pub repo: String,
    /// Free text matched against title and body; `None` matches every PR in scope.
    pub text: Option<String>,
    /// `open` / `closed` / `merged` / `all`.
    pub state: String,
    pub author: Option<String>,
    pub base: Option<String>,
    pub limit: u32,
}

/// One search hit.
///
/// `GET /search/issues` reports no head or base branch, so neither is present here: a caller that
/// needs the branches follows up with [`GithubPrInsightApi::get_pr`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSearchHit {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub draft: bool,
    pub author: String,
    pub url: String,
    pub updated_at: String,
}

/// Reads that let an operator inspect a pull request, plus the one write that keeps a stack node's
/// title and body in step with its PR.
///
/// A **sibling** of [`GithubPrApi`] rather than more methods on it: the eight hand-written fakes
/// that implement `GithubPrApi` today care only about lifecycle operations, and widening that trait
/// would force every one of them to grow stubs for reads it never exercises.
pub trait GithubPrInsightApi: Send + Sync {
    /// `GET /repos/{repo}/pulls/{number}`.
    fn get_pr(&self, number: u64) -> Result<PrDetail, tddy_core::WorkflowError>;

    /// `GET /repos/{repo}/pulls/{number}/files`.
    fn list_pr_files(&self, number: u64) -> Result<Vec<PrFile>, tddy_core::WorkflowError>;

    /// `GET /repos/{repo}/commits/{head_sha}/check-runs`.
    fn list_check_runs(&self, head_sha: &str) -> Result<Vec<CheckRun>, tddy_core::WorkflowError>;

    /// `GET /repos/{repo}/pulls/{number}/reviews`.
    fn list_reviews(&self, number: u64) -> Result<Vec<PrReview>, tddy_core::WorkflowError>;

    /// `GET /repos/{repo}/pulls/{number}/comments` — diff-anchored review comments, flat.
    fn list_review_comments(
        &self,
        number: u64,
    ) -> Result<Vec<PrReviewComment>, tddy_core::WorkflowError>;

    /// `GET /repos/{repo}/issues/{number}/comments` — conversation comments.
    fn list_issue_comments(
        &self,
        number: u64,
    ) -> Result<Vec<PrIssueComment>, tddy_core::WorkflowError>;

    /// `GET /search/issues` with `repo:` and `is:pr` qualifiers.
    fn search_prs(
        &self,
        query: &PrSearchQuery,
    ) -> Result<Vec<PrSearchHit>, tddy_core::WorkflowError>;

    /// `PATCH /repos/{repo}/pulls/{number}` with whichever of title/body is present.
    fn patch_pr_title_body(
        &self,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<(), tddy_core::WorkflowError>;
}

/// Where a [`RealGithubPrApi`] gets its credential. Explicit never falls back to the environment:
/// a caller acting for a specific operator must not silently authenticate as the host's ambient
/// token.
enum TokenSource {
    /// `GITHUB_TOKEN` / `GH_TOKEN` of the running process — correct for the recipe and CLI callers,
    /// which run as the operator.
    ProcessEnv,
    /// A token supplied by the caller (e.g. the daemon, acting for a logged-in web operator).
    Explicit(String),
}

/// Real implementation using GitHub REST API via `curl`.
/// `repo` is `owner/repo` (e.g. `"acme/myrepo"`).
pub struct RealGithubPrApi {
    pub repo: String,
    token: TokenSource,
}

impl RealGithubPrApi {
    /// Authenticate with the process environment (`GITHUB_TOKEN` / `GH_TOKEN`) — the recipe and CLI
    /// callers, which run as the operator.
    pub fn new(repo: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            token: TokenSource::ProcessEnv,
        }
    }

    /// Authenticate with an explicitly supplied token — a server acting for one operator, whose own
    /// credential must be used instead of the host's ambient environment.
    pub fn with_token(repo: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            token: TokenSource::Explicit(token.into()),
        }
    }

    /// Resolve the credential for one call, or an operator-facing reason why there is none.
    fn resolve_token(&self) -> Result<String, String> {
        match &self.token {
            TokenSource::ProcessEnv => crate::github_rest_common::github_token_from_env()
                .ok_or_else(|| "no GitHub token set (GITHUB_TOKEN / GH_TOKEN)".to_string()),
            TokenSource::Explicit(t) if !t.trim().is_empty() => Ok(t.clone()),
            TokenSource::Explicit(_) => Err("the supplied GitHub token is blank".to_string()),
        }
    }

    /// [`Self::resolve_token`] as a `WorkflowError`, for the operations that fail closed.
    fn require_token(&self, op: &str) -> Result<String, tddy_core::WorkflowError> {
        self.resolve_token()
            .map_err(|reason| tddy_core::WorkflowError::WriteFailed(format!("{op}: {reason}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::sync::Mutex;

    struct MockGithubPrApi {
        get_open_pr_calls: Mutex<Vec<String>>,
        close_pr_calls: Mutex<Vec<u64>>,
    }

    impl MockGithubPrApi {
        fn new() -> Self {
            Self {
                get_open_pr_calls: Mutex::new(vec![]),
                close_pr_calls: Mutex::new(vec![]),
            }
        }
    }

    impl GithubPrApi for MockGithubPrApi {
        fn get_open_pr(
            &self,
            head_branch: &str,
        ) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
            self.get_open_pr_calls
                .lock()
                .unwrap()
                .push(head_branch.to_string());
            Ok(Some(PrRef {
                number: 42,
                head_sha: "abc123".to_string(),
                base_branch: "master".to_string(),
                url: "https://github.com/example/repo/pull/42".to_string(),
            }))
        }
        fn get_pr_by_head(&self, _head_branch: &str) -> PrLookupOutcome {
            PrLookupOutcome::Found(PrView {
                number: 42,
                url: "https://github.com/example/repo/pull/42".to_string(),
                state: PrState::Open,
            })
        }
        fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
            Ok("merge-sha-abc".to_string())
        }
        fn patch_pr_base(
            &self,
            _number: u64,
            _new_base: &str,
        ) -> Result<(), tddy_core::WorkflowError> {
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
        fn close_pr(&self, number: u64) -> Result<(), tddy_core::WorkflowError> {
            self.close_pr_calls.lock().unwrap().push(number);
            Ok(())
        }
    }

    #[test]
    fn github_mock_records_close_pr_call() {
        // Given
        let mock = MockGithubPrApi::new();
        let api: &dyn GithubPrApi = &mock;

        // When — close PR #7 without merging
        api.close_pr(7).unwrap();

        // Then
        assert_eq!(mock.close_pr_calls.lock().unwrap().as_slice(), &[7]);
    }

    #[test]
    fn github_mock_records_get_open_pr_call() {
        let mock = MockGithubPrApi::new();
        let result = mock.get_open_pr("feature/n1");
        let pr_ref = result.unwrap().unwrap();
        assert_eq!(pr_ref.number, 42);
        let calls = mock.get_open_pr_calls.lock().unwrap();
        assert_eq!(calls.as_slice(), &["feature/n1".to_string()]);
    }

    // -----------------------------------------------------------------------
    // search_qualifiers — the `q` a PR search is actually run with.
    //
    // PRD: docs/ft/coder/pr-stacking.md § GitHub API surface.
    // Changeset: docs/dev/changesets/2026-07-30-pr-stack-full-control.md.
    // -----------------------------------------------------------------------

    /// The op name a caller passes in; it prefixes any rejection.
    const SEARCH_OP: &str = "RealGithubPrApi::search_prs";

    /// A search of `acme/repo` over every state, with nothing narrowed.
    fn a_search() -> PrSearchQuery {
        PrSearchQuery {
            repo: "acme/repo".to_string(),
            text: None,
            state: "all".to_string(),
            author: None,
            base: None,
            limit: 20,
        }
    }

    fn qualifiers_of(query: PrSearchQuery) -> String {
        search_qualifiers(SEARCH_OP, &query).expect("this search should be accepted")
    }

    #[test]
    fn every_search_is_scoped_to_one_repositorys_pull_requests() {
        // Given — a search that narrows nothing at all
        let query = a_search();

        // When
        let q = qualifiers_of(query);

        // Then — the repository and the pull-request scope are injected, never optional
        assert_eq!(q, "repo:acme/repo is:pr");
    }

    #[rstest]
    #[case::open("open", "repo:acme/repo is:pr is:open")]
    #[case::closed("closed", "repo:acme/repo is:pr is:closed")]
    #[case::merged("merged", "repo:acme/repo is:pr is:merged")]
    #[case::all("all", "repo:acme/repo is:pr")]
    fn each_state_maps_to_githubs_own_state_qualifier(#[case] state: &str, #[case] expected: &str) {
        // Given — one of the four states the tool accepts; "all" narrows nothing
        let query = PrSearchQuery {
            state: state.to_string(),
            ..a_search()
        };

        // When
        let q = qualifiers_of(query);

        // Then
        assert_eq!(q, expected);
    }

    #[test]
    fn an_author_and_a_base_each_add_their_own_qualifier() {
        // Given
        let query = PrSearchQuery {
            author: Some("alice".to_string()),
            base: Some("master".to_string()),
            ..a_search()
        };

        // When
        let q = qualifiers_of(query);

        // Then
        assert_eq!(q, "repo:acme/repo is:pr author:alice base:master");
    }

    #[test]
    fn a_blank_author_adds_no_author_qualifier() {
        // Given — a caller that sent the field but named nobody
        let query = PrSearchQuery {
            author: Some("   ".to_string()),
            ..a_search()
        };

        // When
        let q = qualifiers_of(query);

        // Then — `author:` with nothing after it would match no PR at all
        assert_eq!(q, "repo:acme/repo is:pr");
    }

    #[test]
    fn a_blank_base_adds_no_base_qualifier() {
        // Given
        let query = PrSearchQuery {
            base: Some("".to_string()),
            ..a_search()
        };

        // When
        let q = qualifiers_of(query);

        // Then
        assert_eq!(q, "repo:acme/repo is:pr");
    }

    #[test]
    fn free_text_is_matched_as_written_beside_the_injected_scope() {
        // Given
        let query = PrSearchQuery {
            text: Some("  token store  ".to_string()),
            ..a_search()
        };

        // When
        let q = qualifiers_of(query);

        // Then
        assert_eq!(q, "repo:acme/repo is:pr token store");
    }

    #[test]
    fn an_unknown_state_is_rejected_rather_than_narrowed_to_a_default() {
        // Given — a state GitHub has no `is:` qualifier for
        let query = PrSearchQuery {
            state: "abandoned".to_string(),
            ..a_search()
        };

        // When
        let result = search_qualifiers(SEARCH_OP, &query);

        // Then — answering a different question than the one asked would be worse than refusing
        assert_eq!(
            result.unwrap_err().to_string(),
            "artifact write failed: RealGithubPrApi::search_prs: unknown state 'abandoned' \
             (expected open, closed, merged or all)"
        );
    }

    #[rstest]
    #[case::colon("alice:x")]
    #[case::second_qualifier("alice repo:someone/private")]
    fn an_author_that_is_not_a_single_name_is_rejected(#[case] hostile_author: &str) {
        // Given — a value that would append a qualifier of the caller's choosing to `q`
        let query = PrSearchQuery {
            author: Some(hostile_author.to_string()),
            ..a_search()
        };

        // When
        let result = search_qualifiers(SEARCH_OP, &query);

        // Then — the field the free-text refusal points callers at must not itself be a way in
        assert_eq!(
            result.unwrap_err().to_string(),
            format!(
                "artifact write failed: RealGithubPrApi::search_prs: the author '{hostile_author}' \
                 is not a single name — a search's repository and pull-request scope are set by this \
                 tool and cannot be widened by a qualifier"
            )
        );
    }

    #[test]
    fn a_base_that_is_not_a_single_name_is_rejected() {
        // Given — a git branch name may contain neither a space nor a colon, so this is not a branch
        let query = PrSearchQuery {
            base: Some("master repo:someone/private".to_string()),
            ..a_search()
        };

        // When
        let result = search_qualifiers(SEARCH_OP, &query);

        // Then
        assert_eq!(
            result.unwrap_err().to_string(),
            "artifact write failed: RealGithubPrApi::search_prs: the base 'master \
             repo:someone/private' is not a single name — a search's repository and pull-request \
             scope are set by this tool and cannot be widened by a qualifier"
        );
    }

    #[test]
    fn free_text_carrying_a_search_qualifier_is_rejected() {
        // Given — a `repo:` of the caller's own, which GitHub ORs with the injected one
        let query = PrSearchQuery {
            text: Some("repo:someone/private token".to_string()),
            ..a_search()
        };

        // When
        let result = search_qualifiers(SEARCH_OP, &query);

        // Then — accepting it would read another repository's PRs with the operator's own credential
        assert_eq!(
            result.unwrap_err().to_string(),
            "artifact write failed: RealGithubPrApi::search_prs: the search text 'repo:someone/private \
             token' contains ':', which GitHub reads as a search qualifier — the repository and the \
             pull-request scope are set by this tool, so narrow a search with the state, author and \
             base fields instead"
        );
    }

    #[test]
    fn github_mock_patch_pr_base_and_merge_pr_called_in_sequence() {
        // Verifies that the trait allows both patch_pr_base + merge_pr to be called
        // in sequence on the same &dyn GithubPrApi reference.
        let mock = MockGithubPrApi::new();
        let api: &dyn GithubPrApi = &mock;
        api.patch_pr_base(7, "master").unwrap();
        api.merge_pr(7).unwrap();
        // If GithubPrApi trait is not object-safe or method signatures are wrong,
        // this won't compile. That is the intended compile-time failure.
    }
}

/// Extract `owner/repo` from a git remote URL (SSH or HTTPS).
///
/// Handles:
/// - `git@github.com:owner/repo.git`
/// - `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo`
pub fn owner_repo_from_remote_url(remote_url: &str) -> Option<String> {
    let url = remote_url.trim();
    // SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let owner_repo = rest.trim_end_matches(".git");
        if owner_repo.contains('/') {
            return Some(owner_repo.to_string());
        }
        return None;
    }
    // HTTPS: https://github.com/owner/repo[.git]
    if let Some(rest) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
    {
        let owner_repo = rest.trim_end_matches(".git").trim_end_matches('/');
        if owner_repo.contains('/') {
            return Some(owner_repo.to_string());
        }
        return None;
    }
    None
}

#[cfg(test)]
mod real_impl_tests {
    use super::*;

    /// `real_github_get_open_pr_errors_without_token` — when no GitHub token is set,
    /// `RealGithubPrApi::get_open_pr` must return `Err` immediately (token gating) rather
    /// than calling curl with an empty Authorization header.
    #[test]
    fn real_github_get_open_pr_errors_without_token() {
        let token_backup = (
            std::env::var("GITHUB_TOKEN").ok(),
            std::env::var("GH_TOKEN").ok(),
        );
        unsafe {
            std::env::remove_var("GITHUB_TOKEN");
            std::env::remove_var("GH_TOKEN");
        }

        let api = RealGithubPrApi::new("owner/repo");
        let result = api.get_open_pr("owner:feature/branch");

        if let Some(t) = token_backup.0 {
            unsafe { std::env::set_var("GITHUB_TOKEN", t) };
        }
        if let Some(t) = token_backup.1 {
            unsafe { std::env::set_var("GH_TOKEN", t) };
        }

        assert!(
            result.is_err(),
            "get_open_pr must return Err when no GitHub token is set; got: {result:?}"
        );
    }

    /// `real_github_close_pr_errors_without_token` — `close_pr` must fail closed when no GitHub
    /// token is configured, never issuing a curl PATCH with an empty Authorization header.
    #[test]
    fn real_github_close_pr_errors_without_token() {
        let token_backup = (
            std::env::var("GITHUB_TOKEN").ok(),
            std::env::var("GH_TOKEN").ok(),
        );
        unsafe {
            std::env::remove_var("GITHUB_TOKEN");
            std::env::remove_var("GH_TOKEN");
        }

        let api = RealGithubPrApi::new("owner/repo");
        let result = api.close_pr(7);

        if let Some(t) = token_backup.0 {
            unsafe { std::env::set_var("GITHUB_TOKEN", t) };
        }
        if let Some(t) = token_backup.1 {
            unsafe { std::env::set_var("GH_TOKEN", t) };
        }

        assert!(
            result.is_err(),
            "close_pr must return Err when no GitHub token is set; got: {result:?}"
        );
    }
}

impl GithubPrApi for RealGithubPrApi {
    fn get_open_pr(&self, head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
        let token = self.require_token("RealGithubPrApi::get_open_pr")?;
        // Unqualified, GitHub ignores the filter and returns every open PR — `arr.first()` below
        // would then repoint or merge an arbitrary PR.
        let head = qualified_head(&self.repo, head_branch);
        let body = crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            "pulls",
            &[("state", "open"), ("head", head.as_str())],
            &token,
        )?;
        let items: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            tddy_core::WorkflowError::WriteFailed(format!("get_open_pr: JSON parse error: {e}"))
        })?;
        let arr = items.as_array().ok_or_else(|| {
            tddy_core::WorkflowError::WriteFailed(format!(
                "get_open_pr: expected array, got: {body}"
            ))
        })?;
        let Some(pr) = arr.first() else {
            return Ok(None);
        };
        let number = pr.get("number").and_then(|n| n.as_u64()).ok_or_else(|| {
            tddy_core::WorkflowError::WriteFailed(format!("get_open_pr: missing number in {pr}"))
        })?;
        let head_sha = pr
            .pointer("/head/sha")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let base_branch = pr
            .pointer("/base/ref")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let url = pr
            .get("html_url")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        Ok(Some(PrRef {
            number,
            head_sha,
            base_branch,
            url,
        }))
    }

    fn get_pr_by_head(&self, head_branch: &str) -> PrLookupOutcome {
        let token = match self.resolve_token() {
            Ok(t) => t,
            Err(reason) => return PrLookupOutcome::Unavailable(reason),
        };
        // Unqualified, GitHub ignores the filter and returns the whole PR list.
        let head = qualified_head(&self.repo, head_branch);
        let body = match crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            "pulls",
            &[("state", "all"), ("head", head.as_str())],
            &token,
        ) {
            Ok(b) => b,
            Err(e) => return PrLookupOutcome::Unavailable(e.to_string()),
        };
        let items: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                return PrLookupOutcome::Unavailable(format!(
                    "GitHub returned an unparseable PR list: {e}"
                ));
            }
        };
        let Some(arr) = items.as_array() else {
            return PrLookupOutcome::Unavailable(format!(
                "GitHub returned a PR list that is not an array: {body}"
            ));
        };
        let Some(pr) = arr.first() else {
            return PrLookupOutcome::NotFound;
        };
        let Some(number) = pr.get("number").and_then(|n| n.as_u64()) else {
            return PrLookupOutcome::Unavailable(format!("GitHub PR entry has no number: {pr}"));
        };
        let url = pr
            .get("html_url")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let state = pr.get("state").and_then(|s| s.as_str()).unwrap_or("open");
        let merged_at = pr.get("merged_at").and_then(|s| s.as_str());
        let draft = pr.get("draft").and_then(|d| d.as_bool()).unwrap_or(false);
        PrLookupOutcome::Found(PrView {
            number,
            url,
            state: pr_state_from_github(state, merged_at, draft),
        })
    }

    fn merge_pr(&self, number: u64) -> Result<String, tddy_core::WorkflowError> {
        let token = self.require_token("RealGithubPrApi::merge_pr")?;
        let body = crate::github_rest_common::curl_github_put_json_with_token(
            &self.repo,
            &format!("pulls/{number}/merge"),
            r#"{"merge_method":"merge"}"#,
            &token,
        )?;
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            tddy_core::WorkflowError::WriteFailed(format!("merge_pr: JSON parse: {e}"))
        })?;
        Ok(v.get("sha")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string())
    }

    fn patch_pr_base(&self, number: u64, new_base: &str) -> Result<(), tddy_core::WorkflowError> {
        let token = self.require_token("RealGithubPrApi::patch_pr_base")?;
        let body = serde_json::json!({ "base": new_base }).to_string();
        crate::github_rest_common::curl_github_patch_json_with_token(
            &self.repo,
            &format!("pulls/{number}"),
            &body,
            &token,
        )?;
        Ok(())
    }

    fn create_pr(
        &self,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        let payload = serde_json::json!({
            "head": head,
            "base": base,
            "title": title,
            "body": body,
        })
        .to_string();
        let token = self.require_token("RealGithubPrApi::create_pr")?;
        let resp = crate::github_rest_common::curl_github_post_json_with_token(
            &self.repo, "pulls", &payload, &token,
        )?;
        let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| {
            tddy_core::WorkflowError::WriteFailed(format!("create_pr: JSON parse: {e}"))
        })?;
        v.get("number").and_then(|n| n.as_u64()).ok_or_else(|| {
            tddy_core::WorkflowError::WriteFailed(format!(
                "create_pr: missing number in response: {resp}"
            ))
        })
    }

    fn disable_auto_merge(&self, number: u64) -> Result<(), tddy_core::WorkflowError> {
        // GitHub REST API: DELETE /repos/{repo}/pulls/{number}/merge-queue is not standard;
        // use the GraphQL mutation disablePullRequestAutoMerge — but for simplicity we patch
        // the PR to set auto_merge off via the REST API.
        // If the endpoint is unavailable, we best-effort ignore the error.
        let token = self.require_token("RealGithubPrApi::disable_auto_merge")?;
        let body = serde_json::json!({ "auto_merge": null }).to_string();
        let _ = crate::github_rest_common::curl_github_patch_json_with_token(
            &self.repo,
            &format!("pulls/{number}"),
            &body,
            &token,
        );
        Ok(())
    }

    fn close_pr(&self, number: u64) -> Result<(), tddy_core::WorkflowError> {
        let token = self.require_token("RealGithubPrApi::close_pr")?;
        let body = serde_json::json!({ "state": "closed" }).to_string();
        crate::github_rest_common::curl_github_patch_json_with_token(
            &self.repo,
            &format!("pulls/{number}"),
            &body,
            &token,
        )?;
        Ok(())
    }
}

/// One page's worth of items on a list endpoint — GitHub's maximum, and all these reads take.
///
/// Deliberately un-paginated: a PR with more than a hundred reviews, comments or check runs is
/// outside what an agent can usefully be handed in one tool result, and a silently truncated second
/// page would be indistinguishable from there being no second page.
const PER_PAGE: &str = "100";

fn json_err(op: &str, detail: impl std::fmt::Display) -> tddy_core::WorkflowError {
    tddy_core::WorkflowError::WriteFailed(format!("{op}: {detail}"))
}

/// Parse a response body that must be a JSON array.
fn json_array(op: &str, body: &str) -> Result<Vec<serde_json::Value>, tddy_core::WorkflowError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| json_err(op, format!("JSON parse error: {e}")))?;
    match value {
        serde_json::Value::Array(items) => Ok(items),
        other => Err(json_err(op, format!("expected a JSON array, got: {other}"))),
    }
}

/// A string field, or `""` when GitHub reports it as `null` (an empty PR body, a check run with no
/// conclusion yet). Distinguishing "absent" from "empty" would not change any caller's decision.
fn json_str(item: &serde_json::Value, field: &str) -> String {
    item.get(field)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// A nested string field addressed by JSON pointer, with the same "`null` reads as `""`" rule as
/// [`json_str`].
fn json_pointer(item: &serde_json::Value, path: &str) -> String {
    item.pointer(path)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// The author login of an item that carries a `user` object; `""` for a comment whose author's
/// account is gone (GitHub reports `null` there).
fn json_author(item: &serde_json::Value) -> String {
    json_pointer(item, "/user/login")
}

impl GithubPrInsightApi for RealGithubPrApi {
    fn get_pr(&self, number: u64) -> Result<PrDetail, tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::get_pr";
        let token = self.require_token(OP)?;
        let body = crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            &format!("pulls/{number}"),
            &[],
            &token,
        )?;
        let pr: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| json_err(OP, format!("JSON parse error: {e}")))?;

        let number = pr
            .get("number")
            .and_then(|n| n.as_u64())
            .ok_or_else(|| json_err(OP, format!("missing number in response: {body}")))?;
        // No default: `state` decides what `pr_read` reports and what phase an adopted node is
        // created at, so reading a missing field as "open" would present a merged or closed PR as
        // still in play.
        let state = pr
            .get("state")
            .and_then(|s| s.as_str())
            .ok_or_else(|| json_err(OP, format!("missing state in response: {body}")))?;
        let merged_at = pr.get("merged_at").and_then(|s| s.as_str());
        let draft = pr.get("draft").and_then(|d| d.as_bool()).unwrap_or(false);

        Ok(PrDetail {
            number,
            url: json_str(&pr, "html_url"),
            title: json_str(&pr, "title"),
            body: json_str(&pr, "body"),
            state: pr_state_from_github(state, merged_at, draft),
            base_branch: json_pointer(&pr, "/base/ref"),
            head_branch: json_pointer(&pr, "/head/ref"),
            head_sha: json_pointer(&pr, "/head/sha"),
            // `null` while GitHub is still computing mergeability — kept as `None` rather than
            // collapsed to `false`, which would read as "conflicted".
            mergeable: pr.get("mergeable").and_then(|m| m.as_bool()),
            mergeable_state: json_str(&pr, "mergeable_state"),
            additions: pr.get("additions").and_then(|n| n.as_u64()).unwrap_or(0),
            deletions: pr.get("deletions").and_then(|n| n.as_u64()).unwrap_or(0),
            changed_files: pr
                .get("changed_files")
                .and_then(|n| n.as_u64())
                .unwrap_or(0),
        })
    }

    fn list_pr_files(&self, number: u64) -> Result<Vec<PrFile>, tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::list_pr_files";
        let token = self.require_token(OP)?;
        let body = crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            &format!("pulls/{number}/files"),
            &[("per_page", PER_PAGE)],
            &token,
        )?;
        Ok(json_array(OP, &body)?
            .iter()
            .map(|file| PrFile {
                path: json_str(file, "filename"),
                status: json_str(file, "status"),
            })
            .collect())
    }

    fn list_check_runs(&self, head_sha: &str) -> Result<Vec<CheckRun>, tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::list_check_runs";
        let token = self.require_token(OP)?;
        let body = crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            &format!("commits/{head_sha}/check-runs"),
            &[("per_page", PER_PAGE)],
            &token,
        )?;
        let response: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| json_err(OP, format!("JSON parse error: {e}")))?;
        // Unlike the other list endpoints, this one wraps its items in an object.
        let runs = response
            .get("check_runs")
            .and_then(|r| r.as_array())
            .ok_or_else(|| json_err(OP, format!("missing check_runs array in: {body}")))?;
        Ok(runs
            .iter()
            .map(|run| CheckRun {
                name: json_str(run, "name"),
                // `null` while the run is still going: reported as empty, never as a failure.
                conclusion: json_str(run, "conclusion"),
            })
            .collect())
    }

    fn list_reviews(&self, number: u64) -> Result<Vec<PrReview>, tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::list_reviews";
        let token = self.require_token(OP)?;
        let body = crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            &format!("pulls/{number}/reviews"),
            &[("per_page", PER_PAGE)],
            &token,
        )?;
        Ok(json_array(OP, &body)?
            .iter()
            .map(|review| PrReview {
                author: json_author(review),
                state: json_str(review, "state"),
                body: json_str(review, "body"),
                submitted_at: json_str(review, "submitted_at"),
            })
            .collect())
    }

    fn list_review_comments(
        &self,
        number: u64,
    ) -> Result<Vec<PrReviewComment>, tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::list_review_comments";
        let token = self.require_token(OP)?;
        let body = crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            &format!("pulls/{number}/comments"),
            &[("per_page", PER_PAGE)],
            &token,
        )?;
        let items = json_array(OP, &body)?;
        let mut comments = Vec::with_capacity(items.len());
        for comment in &items {
            // Without an id, a comment cannot be placed in a thread and no reply could ever be
            // attached to it — a silently id-less comment would break the grouping it belongs to.
            let id = comment
                .get("id")
                .and_then(|n| n.as_u64())
                .ok_or_else(|| json_err(OP, format!("review comment has no id: {comment}")))?;
            comments.push(PrReviewComment {
                id,
                in_reply_to_id: comment.get("in_reply_to_id").and_then(|n| n.as_u64()),
                author: json_author(comment),
                body: json_str(comment, "body"),
                path: json_str(comment, "path"),
                // `null` once the diff position it was anchored to has gone stale.
                line: comment.get("line").and_then(|n| n.as_u64()),
                diff_hunk: json_str(comment, "diff_hunk"),
                created_at: json_str(comment, "created_at"),
            });
        }
        Ok(comments)
    }

    fn list_issue_comments(
        &self,
        number: u64,
    ) -> Result<Vec<PrIssueComment>, tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::list_issue_comments";
        let token = self.require_token(OP)?;
        // A PR's conversation comments live on its issue, not on the pull resource.
        let body = crate::github_rest_common::curl_github_get_json_with_token(
            &self.repo,
            &format!("issues/{number}/comments"),
            &[("per_page", PER_PAGE)],
            &token,
        )?;
        Ok(json_array(OP, &body)?
            .iter()
            .map(|comment| PrIssueComment {
                author: json_author(comment),
                body: json_str(comment, "body"),
                created_at: json_str(comment, "created_at"),
            })
            .collect())
    }

    fn search_prs(
        &self,
        query: &PrSearchQuery,
    ) -> Result<Vec<PrSearchHit>, tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::search_prs";
        let token = self.require_token(OP)?;
        let q = search_qualifiers(OP, query)?;
        let per_page = query.limit.to_string();
        // `/search/issues` is not under `/repos/{owner}/{repo}/` — the repository is a `repo:`
        // qualifier inside `q`, which is why this call needs the absolute-path helper.
        let body = crate::github_rest_common::curl_github_get_json_absolute_path(
            "search/issues",
            &[("q", q.as_str()), ("per_page", per_page.as_str())],
            &token,
        )?;
        let response: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| json_err(OP, format!("JSON parse error: {e}")))?;
        let items = response
            .get("items")
            .and_then(|i| i.as_array())
            .ok_or_else(|| json_err(OP, format!("missing items array in: {body}")))?;

        let mut hits = Vec::with_capacity(items.len());
        for item in items {
            let number = item
                .get("number")
                .and_then(|n| n.as_u64())
                .ok_or_else(|| json_err(OP, format!("search hit has no number: {item}")))?;
            hits.push(PrSearchHit {
                number,
                title: json_str(item, "title"),
                state: json_str(item, "state"),
                draft: item.get("draft").and_then(|d| d.as_bool()).unwrap_or(false),
                author: json_author(item),
                url: json_str(item, "html_url"),
                updated_at: json_str(item, "updated_at"),
            });
        }
        Ok(hits)
    }

    fn patch_pr_title_body(
        &self,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<(), tddy_core::WorkflowError> {
        const OP: &str = "RealGithubPrApi::patch_pr_title_body";
        // Only the fields the caller named are sent: restating an unchanged body would overwrite
        // whatever had been edited on GitHub in the meantime.
        let mut payload = serde_json::Map::new();
        if let Some(title) = title {
            payload.insert("title".to_string(), serde_json::Value::from(title));
        }
        if let Some(body) = body {
            payload.insert("body".to_string(), serde_json::Value::from(body));
        }
        if payload.is_empty() {
            // An empty PATCH would be a request that cannot have been meant; reporting it beats
            // spending a round trip to change nothing and calling that success.
            return Err(json_err(
                OP,
                format!("neither a title nor a body was given for PR #{number}"),
            ));
        }
        // A named title must carry something: GitHub answers `{"title": ""}` with a 422, because a
        // pull request cannot be untitled. Refused here rather than trimmed away — dropping the field
        // would apply half of the call and report the whole of it as success — and refusing before the
        // round trip names the field instead of leaving the operator to read a 422 from
        // api.github.com.
        //
        // A blank *body* is deliberately allowed. GitHub's `body` is nullable and accepts `""`, so
        // clearing a stale description is a legitimate edit; refusing it would make this surface the
        // only one that cannot.
        if title.is_some_and(|title| title.trim().is_empty()) {
            return Err(json_err(
                OP,
                format!(
                    "the title given for PR #{number} is blank — name a title, or leave the title \
                     out of the edit"
                ),
            ));
        }

        let token = self.require_token(OP)?;
        crate::github_rest_common::curl_github_patch_json_with_token(
            &self.repo,
            &format!("pulls/{number}"),
            &serde_json::Value::Object(payload).to_string(),
            &token,
        )?;
        Ok(())
    }
}

/// One caller-supplied qualifier value, checked to be a single bare word.
///
/// `author:` and `base:` values are interpolated into `q` beside the injected `repo:` and `is:pr`,
/// so a value carrying whitespace or a `:` would append qualifiers of the caller's choosing — and
/// GitHub *ORs* repeated `repo:` qualifiers, which is how a search scoped to one repository would
/// come to read another one with the operator's own credential. Neither a GitHub login nor a git
/// branch name may contain either character, so no legitimate value is refused here.
fn scoped_value<'v>(
    op: &str,
    field: &str,
    value: &'v str,
) -> Result<&'v str, tddy_core::WorkflowError> {
    let value = value.trim();
    if value.contains(':') || value.split_whitespace().count() > 1 {
        return Err(json_err(
            op,
            format!(
                "the {field} '{value}' is not a single name — a search's repository and \
                 pull-request scope are set by this tool and cannot be widened by a qualifier"
            ),
        ));
    }
    Ok(value)
}

/// Build the `q` value for `/search/issues` from a [`PrSearchQuery`].
///
/// `repo:` and `is:pr` are always injected, so a search can only ever read this repository's pull
/// requests — which is also why free text carrying a `:` is rejected: it would be a qualifier of the
/// caller's own, and a second `repo:` widens the search rather than narrowing it. An unrecognised
/// `state` is rejected rather than defaulting to one — quietly searching open PRs when the caller
/// asked for something else would answer a different question than the one it was asked.
///
/// `closed` maps to GitHub's `is:closed`, which by its own definition also matches merged PRs; ask
/// for `merged` when only merged ones are wanted.
fn search_qualifiers(op: &str, query: &PrSearchQuery) -> Result<String, tddy_core::WorkflowError> {
    let mut qualifiers = vec![format!("repo:{}", query.repo), "is:pr".to_string()];
    match query.state.as_str() {
        "open" => qualifiers.push("is:open".to_string()),
        "closed" => qualifiers.push("is:closed".to_string()),
        "merged" => qualifiers.push("is:merged".to_string()),
        // Every state — no qualifier narrows it.
        "all" => {}
        other => {
            return Err(json_err(
                op,
                format!("unknown state '{other}' (expected open, closed, merged or all)"),
            ));
        }
    }
    if let Some(author) = query.author.as_deref().filter(|a| !a.trim().is_empty()) {
        qualifiers.push(format!("author:{}", scoped_value(op, "author", author)?));
    }
    if let Some(base) = query.base.as_deref().filter(|b| !b.trim().is_empty()) {
        qualifiers.push(format!("base:{}", scoped_value(op, "base", base)?));
    }
    if let Some(text) = query.text.as_deref().filter(|t| !t.trim().is_empty()) {
        let text = text.trim();
        // Free text is joined into `q` beside the injected qualifiers, and GitHub *ORs* repeated
        // `repo:` qualifiers — so `repo:someone/private token` would return pull requests from a
        // repository this surface promises it cannot reach, read with the operator's own credential.
        // Rejected rather than quoted: turning the caller's words into a phrase would silently change
        // what the search matches.
        if text.contains(':') {
            return Err(json_err(
                op,
                format!(
                    "the search text '{text}' contains ':', which GitHub reads as a search \
                     qualifier — the repository and the pull-request scope are set by this tool, so \
                     narrow a search with the state, author and base fields instead"
                ),
            ));
        }
        qualifiers.push(text.to_string());
    }
    Ok(qualifiers.join(" "))
}
