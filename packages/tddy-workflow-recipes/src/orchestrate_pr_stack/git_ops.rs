//! Git operations for orchestrate-pr-stack: rebase, force-push, merge-base, integration refs.

// TODO: implement — git rebase --onto <new_base> <old_base> <branch>; enable rerere; detect conflicts → WorkflowError
/// Rebase `branch` onto `new_base`, replacing `old_base` as the fork point.
#[allow(dead_code)]
pub fn rebase_onto(
    _repo_root: &std::path::Path,
    _new_base: &str,
    _old_base: &str,
    _branch: &str,
) -> Result<(), tddy_core::WorkflowError> {
    unimplemented!("rebase_onto: not yet implemented")
}

// TODO: implement — git push --force-with-lease=<branch>:<expected_sha> origin <branch>
/// Force-push `branch` to origin, aborting if origin no longer matches `expected_sha`.
#[allow(dead_code)]
pub fn force_push_with_lease(
    _repo_root: &std::path::Path,
    _branch: &str,
    _expected_sha: &str,
) -> Result<(), tddy_core::WorkflowError> {
    unimplemented!("force_push_with_lease: not yet implemented")
}

// TODO: implement — git merge-base a b; used as old_base fallback guard in rebase_onto
/// Compute `git merge-base a b`, returning the common ancestor SHA.
#[allow(dead_code)]
pub fn merge_base(
    _repo_root: &std::path::Path,
    _a: &str,
    _b: &str,
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

// TODO: implement — octopus merge of parent_branches into stack-int/<node_id>; used for multi-parent DAG nodes
/// Build or refresh a local integration ref (`stack-int/<node_id>`) from multiple parent tips.
/// Returns the SHA of the resulting ref.
#[allow(dead_code)]
pub fn build_integration_ref(
    _repo_root: &std::path::Path,
    _node_id: &str,
    _parent_branches: &[String],
) -> Result<String, tddy_core::WorkflowError> {
    unimplemented!("build_integration_ref: not yet implemented")
}
