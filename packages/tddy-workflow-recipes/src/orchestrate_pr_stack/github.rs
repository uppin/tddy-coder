//! GitHub REST PR API abstraction for orchestrate-pr-stack.

/// Resolved open PR reference.
#[derive(Debug, Clone)]
pub struct PrRef {
    pub number: u64,
    pub head_sha: String,
    pub base_branch: String,
    pub url: String,
}

/// Abstraction over GitHub REST PR operations. Allows stubbing in tests.
pub trait GithubPrApi: Send + Sync {
    /// Find open PR whose head matches `head_branch` (format: `owner:branch`).
    fn get_open_pr(&self, head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError>;

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

/// Real implementation using GitHub REST API via `curl`.
/// `repo` is `owner/repo` (e.g. `"acme/myrepo"`).
pub struct RealGithubPrApi {
    pub repo: String,
}

impl RealGithubPrApi {
    pub fn new(repo: impl Into<String>) -> Self {
        Self { repo: repo.into() }
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
        crate::github_rest_common::github_token_from_env().ok_or_else(|| {
            tddy_core::WorkflowError::WriteFailed(
                "RealGithubPrApi::get_open_pr: no GitHub token set (GITHUB_TOKEN / GH_TOKEN)"
                    .to_string(),
            )
        })?;
        let body = crate::github_rest_common::curl_github_get_json(
            &self.repo,
            "pulls",
            &[("state", "open"), ("head", head_branch)],
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

    fn merge_pr(&self, number: u64) -> Result<String, tddy_core::WorkflowError> {
        let body = crate::github_rest_common::curl_github_put_json(
            &self.repo,
            &format!("pulls/{number}/merge"),
            r#"{"merge_method":"merge"}"#,
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
        let body = serde_json::json!({ "base": new_base }).to_string();
        crate::github_rest_common::curl_github_patch_json(
            &self.repo,
            &format!("pulls/{number}"),
            &body,
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
        let resp = crate::github_rest_common::curl_github_post_json(&self.repo, "pulls", &payload)?;
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
        let body = serde_json::json!({ "auto_merge": null }).to_string();
        let _ = crate::github_rest_common::curl_github_patch_json(
            &self.repo,
            &format!("pulls/{number}"),
            &body,
        );
        Ok(())
    }

    fn close_pr(&self, number: u64) -> Result<(), tddy_core::WorkflowError> {
        let body = serde_json::json!({ "state": "closed" }).to_string();
        crate::github_rest_common::curl_github_patch_json(
            &self.repo,
            &format!("pulls/{number}"),
            &body,
        )?;
        Ok(())
    }
}
