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
    }

    impl MockGithubPrApi {
        fn new() -> Self {
            Self { get_open_pr_calls: Mutex::new(vec![]) }
        }
    }

    impl GithubPrApi for MockGithubPrApi {
        fn get_open_pr(&self, head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
            self.get_open_pr_calls.lock().unwrap().push(head_branch.to_string());
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
        fn patch_pr_base(&self, _number: u64, _new_base: &str) -> Result<(), tddy_core::WorkflowError> {
            Ok(())
        }
        fn create_pr(&self, _head: &str, _base: &str, _title: &str, _body: &str) -> Result<u64, tddy_core::WorkflowError> {
            Ok(99)
        }
        fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
            Ok(())
        }
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

impl GithubPrApi for RealGithubPrApi {
    fn get_open_pr(&self, _head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
        unimplemented!("RealGithubPrApi::get_open_pr")
    }

    fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
        unimplemented!("RealGithubPrApi::merge_pr")
    }

    fn patch_pr_base(
        &self,
        _number: u64,
        _new_base: &str,
    ) -> Result<(), tddy_core::WorkflowError> {
        unimplemented!("RealGithubPrApi::patch_pr_base")
    }

    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        unimplemented!("RealGithubPrApi::create_pr")
    }

    fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        unimplemented!("RealGithubPrApi::disable_auto_merge")
    }
}
