//! Git operations for **merge-pr** (fetch `origin`, merge default branch into feature, push).
//!
//! RED skeleton: entry points return errors until the Green phase implements real git IO.

/// Configuration for sync (session worktree path, remote name, default branch — wired in Green).
#[derive(Debug, Clone, Default)]
pub struct MergePrGitConfig {
    /// Session / worktree root; Green will read `git` state from here.
    pub session_worktree: Option<std::path::PathBuf>,
}

/// Outcome of a successful sync (for structured reporting / tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePrSyncReport {
    pub strategy: &'static str,
}

/// Fetch `origin`, merge **`main`** into `HEAD`, ensure clean index (Green).
///
/// RED: always returns an error so granular tests stay failing until implemented.
pub fn sync_feature_with_origin_main(
    _config: &MergePrGitConfig,
) -> Result<MergePrSyncReport, String> {
    Err("merge-pr RED skeleton: sync_feature_with_origin_main not implemented".to_string())
}

/// Fail closed when the index still has unmerged paths (Green: inspect `git ls-files -u`).
pub fn ensure_no_unmerged_paths(_repo_root: &std::path::Path) -> Result<(), String> {
    Err("merge-pr RED skeleton: ensure_no_unmerged_paths not implemented".to_string())
}
