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
