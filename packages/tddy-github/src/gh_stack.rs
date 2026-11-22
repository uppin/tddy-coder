//! `gh stack` membership (`#workqueue` 9/14) — optional: read only where the host's `gh` has the
//! `gh-stack` extension; without it the lineage is complete except for this annotation.

use std::path::Path;

/// The branches of the GitHub stack `branch` belongs to, bottom first; `None` when the extension is
/// missing, the branch is in no stack, or `gh` fails.
pub async fn gh_stack_membership(
    gh_bin: &Path,
    repo_dir: &Path,
    branch: &str,
) -> Option<Vec<String>> {
    // TODO(n9 lineage-graph): implement
    let _ = (gh_bin, repo_dir, branch);
    todo!("TODO(n9): implement gh_stack_membership")
}
