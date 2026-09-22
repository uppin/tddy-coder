//! Repointing a node onto a new base, and realigning its branch and PR with its recorded parents.

use std::path::Path;

use tddy_core::changeset::StackNode;

use super::order::assign_missing_display_order;

/// Repoint a single planned node onto a new base.
///
/// Which parents survive depends on whether the caller names a target:
///
/// - `Some(target)` — retain exactly the parents whose `branch` is `target`, drop every other
///   parent. This is a retain rule, so a target that no parent owns drops all of them and the
///   node detaches onto `default_branch`. It is what makes a node stranded behind a merged-and-
///   deleted predecessor recoverable: that predecessor is still recorded as `open` in the plan
///   (the orchestrator agent writes `pr_status`), so no merged-parents rule could ever drop it.
/// - `None` — retain the parents that are not merged, i.e. drop merged parents only. The
///   behaviour for callers that do not name a target, such as the agent repoint.
///
/// The parent change is persisted atomically. The effective base branch (the nearest remaining
/// non-merged ancestor's branch, or `default_branch` when none remains) is then computed, the
/// node's local branch is rebased onto it and force-pushed, and the open GitHub PR's base is
/// re-targeted to it. Mirrors `bridge::execute_stack_repoint` applied to one node so the web
/// Repoint control and that agent path stay coherent. When the branch is not local (remote-only),
/// the git rebase is skipped and the PR base is still re-targeted.
///
/// `Some(target)` **collapses the node to a single parent** — the one owning `target` — or to none
/// when no parent owns it. Repointing is a decision to stack on one predecessor, so a multi-parent
/// node comes out of it single-parent by design; the other edges are dropped, not preserved.
///
/// `None` is the in-process drop-merged-parents mode. It is not reachable over the wire: the daemon
/// substitutes the project's resolved default branch for an empty `target_base_branch`, because a
/// client cannot always name that branch and forwarding the empty string would silently select this
/// different rule.
///
/// A node that owns no branch is a **plan-only** repoint: the parent change is persisted and the
/// updated node returned, with no rebase, no force-push and no PR re-target. There is nothing to
/// rebase and no PR of its own to re-target — and an unstarted node is precisely the one this
/// recovery exists for.
pub fn repoint_planned_pr_node(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    default_branch: &str,
    target_base_branch: Option<&str>,
    gh: &dyn tddy_github::pr_api::GithubPrApi,
) -> Result<StackNode, String> {
    use tddy_core::changeset::{read_changeset, update_stack_atomic};

    let changeset = read_changeset(session_dir)
        .map_err(|e| format!("repoint_planned_pr_node: failed to read changeset: {e}"))?;
    let stack = changeset.stack.unwrap_or_default();
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("repoint_planned_pr_node: node '{node_id}' not found"))?
        .clone();

    // Which of the node's parents survive the repoint.
    //
    // Decided *inside* the `update_stack_atomic` closure, against the stack that is about to be
    // written. `update_stack_atomic` re-reads the file before applying its closure, and the
    // orchestrator agent writes the same file, so a set computed from the snapshot above would be
    // stale: a keep-list drops any parent added between the two reads, where the drop-list this
    // replaced would have kept it.
    let survives = |stack: &tddy_core::changeset::Stack, parent_id: &str| match target_base_branch {
        // A retain rule: only the parents that own the target base branch stay. A repoint therefore
        // *collapses* the node onto that one predecessor — or detaches it onto the default branch
        // when no parent owns the target, which is what a stranded node needs.
        Some(target) => stack
            .node(parent_id)
            .is_some_and(|p| p.branch.as_deref() == Some(target)),
        // No target named: drop only the parents that are known to have merged. Written as "not
        // known-merged" rather than "resolvable and not merged" so an unresolvable parent id is
        // kept, exactly as the drop-list form this replaced did.
        None => !stack.node(parent_id).is_some_and(|p| p.is_skipped()),
    };

    update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        let retained: Vec<String> = stack
            .node(node_id)
            .map(|n| {
                n.parents
                    .iter()
                    .filter(|parent_id| survives(stack, parent_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.parents = retained;
        }
    })
    .map_err(|e| format!("repoint_planned_pr_node: failed to persist stack: {e}"))?;

    // A node that owns no branch is a plan-only repoint: the persisted parent change above is the
    // whole effect, since there is nothing to rebase and no pull request of its own to re-target.
    if let Some(branch) = node.branch.as_deref() {
        realign_node_to_effective_base(
            "repoint_planned_pr_node",
            session_dir,
            repo_root,
            node_id,
            branch,
            default_branch,
            gh,
        )?;
    }

    let final_stack = read_changeset(session_dir)
        .map_err(|e| format!("repoint_planned_pr_node: failed to reload node: {e}"))?
        .stack
        .unwrap_or_default();
    final_stack
        .node(node_id)
        .cloned()
        .ok_or_else(|| format!("repoint_planned_pr_node: node '{node_id}' vanished after repoint"))
}

/// Bring a branch-owning node's git branch and GitHub PR in line with the parents now recorded for
/// it: rebase onto the new effective base, force-push with lease, re-target the open PR's base.
///
/// Reads the stack fresh rather than taking a base from the caller — the effective base is derived
/// from the parents that were *just written*, and a value computed before that write could name a
/// parent the write dropped.
///
/// A rebase conflict is recorded on the node as `pr_status.phase = "error"` carrying the git message
/// and then returned as an error: the branch is left mid-conflict for a human, and re-targeting the
/// PR to a base the branch does not sit on would misdescribe reality. When the branch is not local
/// (remote-only) the git half is skipped and the PR is still re-targeted.
///
/// Shared by [`repoint_planned_pr_node`] and [`set_stack_node_parents`], which differ only in how
/// they decide the parents — once the DAG is written, making reality match it is the same operation.
///
/// `op` is the caller's own name and prefixes every message this emits. They reach the operator
/// through the daemon, and naming a private helper the operator never invoked would describe the
/// failure of something they did not ask for.
///
/// [`set_stack_node_parents`]: super::set_stack_node_parents
pub(super) fn realign_node_to_effective_base(
    op: &str,
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    branch: &str,
    default_branch: &str,
    gh: &dyn tddy_github::pr_api::GithubPrApi,
) -> Result<(), String> {
    use crate::git_ops::{force_push_with_lease, local_branch_exists, merge_base, rebase_onto};
    use tddy_core::changeset::read_changeset;

    // Effective base after the parent change: strip the `origin/` prefix so it names a branch usable
    // both as a rebase target and a GitHub PR base.
    let updated = read_changeset(session_dir)
        .map_err(|e| format!("{op}: failed to re-read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let base_ref = updated
        .effective_base_refs(node_id, default_branch)
        .into_iter()
        .next()
        .unwrap_or_else(|| default_branch.to_string());
    let effective_base = base_ref
        .strip_prefix("origin/")
        .unwrap_or(&base_ref)
        .to_string();

    // Rebase + force-push only when the branch is local; remote-only branches skip git ops.
    if local_branch_exists(repo_root, branch) {
        let old_base = merge_base(repo_root, branch, &effective_base)
            .unwrap_or_else(|_| effective_base.clone());
        if let Err(e) = rebase_onto(repo_root, &effective_base, &old_base, branch) {
            let err_msg = e.to_string();
            record_rebase_error(op, session_dir, node_id, &err_msg)?;
            return Err(format!(
                "{op}: rebase of {branch} onto {effective_base} failed: {err_msg}"
            ));
        }
        let expected_sha = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(["rev-parse", branch])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        let remote =
            tddy_git::detect_default_remote_name(repo_root).unwrap_or_else(|| "origin".to_string());
        if let Err(e) = force_push_with_lease(repo_root, &remote, branch, &expected_sha) {
            log::warn!("{op}: force-push failed for {branch}: {e}");
        }
    }

    // Re-target the open PR's base to the effective base.
    if let Some(pr) = gh
        .get_open_pr(branch)
        .map_err(|e| format!("{op}: get_open_pr failed: {e}"))?
    {
        gh.patch_pr_base(pr.number, &effective_base)
            .map_err(|e| format!("{op}: patch_pr_base failed: {e}"))?;
    }
    Ok(())
}

/// Record a failed rebase on the node as `pr_status.phase = "error"` carrying the git message.
///
/// The branch is left mid-conflict for a human to finish, so the node has to say so: the stack is the
/// only place the operator will see it, and a node that still read `open` would describe a rebase that
/// succeeded.
fn record_rebase_error(
    op: &str,
    session_dir: &Path,
    node_id: &str,
    err_msg: &str,
) -> Result<(), String> {
    tddy_core::changeset::update_stack_atomic(session_dir, |stack| {
        assign_missing_display_order(stack);
        if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
            node.pr_status = Some(tddy_core::changeset::GithubPrStatus {
                phase: "error".to_string(),
                url: None,
                error: Some(err_msg.to_string()),
            });
        }
    })
    .map_err(|e| format!("{op}: failed to record error: {e}"))
}
