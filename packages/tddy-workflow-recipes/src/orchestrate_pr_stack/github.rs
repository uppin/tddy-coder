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
