//! On-demand PR-management actions invoked by the `orchestrate` goal's `pr_*` tools. These replace
//! the removed autonomous merge/repoint loop: the agent calls a tool, which resolves the node in
//! the orchestrator changeset and runs one of these actions against GitHub / git.

use std::path::Path;
use std::process::Command;

use tddy_core::changeset::{update_stack_atomic, GithubPrStatus};
use tddy_core::WorkflowError;

use super::github::GithubPrApi;

/// Set the persisted `pr_status.phase` of one node, preserving any existing URL.
fn set_node_phase(
    orchestrator_dir: &Path,
    node_id: &str,
    phase: &str,
) -> Result<(), WorkflowError> {
    update_stack_atomic(orchestrator_dir, |stack| {
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            match node.pr_status.as_mut() {
                Some(status) => status.phase = phase.to_string(),
                None => {
                    node.pr_status = Some(GithubPrStatus {
                        phase: phase.to_string(),
                        url: None,
                        error: None,
                    })
                }
            }
        }
    })
}

/// Merge a node's PR into its base and mark the node merged. Returns the merge commit SHA.
pub fn pr_merge_action(
    orchestrator_dir: &Path,
    api: &dyn GithubPrApi,
    node_id: &str,
    pr_number: u64,
) -> Result<String, WorkflowError> {
    let sha = api.merge_pr(pr_number)?;
    set_node_phase(orchestrator_dir, node_id, "merged")?;
    Ok(sha)
}

/// Close a node's PR without merging and mark the node closed.
pub fn pr_close_action(
    orchestrator_dir: &Path,
    api: &dyn GithubPrApi,
    node_id: &str,
    pr_number: u64,
) -> Result<(), WorkflowError> {
    api.close_pr(pr_number)?;
    set_node_phase(orchestrator_dir, node_id, "closed")
}

fn git(worktree_dir: &Path, args: &[&str]) -> Result<std::process::Output, WorkflowError> {
    Command::new("git")
        .args(args)
        .current_dir(worktree_dir)
        .output()
        .map_err(|e| WorkflowError::WriteFailed(format!("git {args:?} failed to run: {e}")))
}

/// Probe a node's worktree branch for conflicts against `base_ref` and report the conflicting files.
///
/// This is **detect-only**: it starts a no-commit merge, collects the unmerged paths, then aborts —
/// it never lands a resolution. The agent resolves the reported files in the worktree itself
/// (edit + `git add` + commit the merge) and re-runs to confirm none remain.
///
/// A `git merge` that conflicts exits non-zero *and* leaves unmerged paths — that is the normal
/// "conflicts found" case. A `git merge` that refuses to start (dirty worktree, unknown ref, …)
/// exits non-zero with *no* unmerged paths; that is reported as an error rather than silently
/// masquerading as "clean".
pub fn pr_resolve_conflicts_action(
    worktree_dir: &Path,
    base_ref: &str,
) -> Result<Vec<String>, WorkflowError> {
    let merge = git(worktree_dir, &["merge", "--no-commit", "--no-ff", base_ref])?;

    let unmerged = git(worktree_dir, &["diff", "--name-only", "--diff-filter=U"])?;
    let mut paths: Vec<String> = String::from_utf8_lossy(&unmerged.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    paths.sort();
    paths.dedup();

    // Leave the worktree clean regardless of outcome (no-op when there was nothing to merge).
    let _ = git(worktree_dir, &["merge", "--abort"]);

    if paths.is_empty() && !merge.status.success() {
        return Err(WorkflowError::WriteFailed(format!(
            "git merge {base_ref} could not start (not a conflict): {}",
            String::from_utf8_lossy(&merge.stderr).trim()
        )));
    }

    Ok(paths)
}
