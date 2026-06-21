//! Git operations for orchestrate-pr-stack: rebase, force-push, merge-base, integration refs.

/// Rebase `branch` onto `new_base`, replacing `old_base` as the fork point.
pub fn rebase_onto(
    repo_root: &std::path::Path,
    new_base: &str,
    old_base: &str,
    branch: &str,
) -> Result<(), tddy_core::WorkflowError> {
    unimplemented!("rebase_onto: not yet implemented")
}

/// Force-push `branch` to origin, aborting if origin no longer matches `expected_sha`.
pub fn force_push_with_lease(
    repo_root: &std::path::Path,
    branch: &str,
    expected_sha: &str,
) -> Result<(), tddy_core::WorkflowError> {
    unimplemented!("force_push_with_lease: not yet implemented")
}

/// Compute `git merge-base a b`, returning the common ancestor SHA.
pub fn merge_base(
    repo_root: &std::path::Path,
    a: &str,
    b: &str,
) -> Result<String, tddy_core::WorkflowError> {
    unimplemented!("merge_base: not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "rebase_onto")]
    fn rebase_onto_panics_unimplemented() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        rebase_onto(tmp.path(), "main", "old-base", "feature").unwrap();
    }

    #[test]
    #[should_panic(expected = "force_push_with_lease")]
    fn force_push_with_lease_panics_unimplemented() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        force_push_with_lease(tmp.path(), "feature", "deadbeef").unwrap();
    }

    #[test]
    #[should_panic(expected = "merge_base")]
    fn merge_base_panics_unimplemented() {
        let tmp = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        merge_base(tmp.path(), "HEAD", "main").unwrap();
    }
}

/// Build or refresh a local integration ref (`stack-int/<node_id>`) from multiple parent tips.
/// Returns the SHA of the resulting ref.
pub fn build_integration_ref(
    repo_root: &std::path::Path,
    node_id: &str,
    parent_branches: &[String],
) -> Result<String, tddy_core::WorkflowError> {
    unimplemented!("build_integration_ref: not yet implemented")
}
