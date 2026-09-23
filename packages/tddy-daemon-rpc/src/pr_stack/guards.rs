//! The refusals a PR-stack mutation runs before it touches an orchestrator's plan.

use tddy_rpc::Status;

/// Guard for any RPC that mutates a `"pr-stack"` orchestrator's `Changeset.stack`: rejects a
/// session whose recipe (or legacy alias) doesn't resolve to `"pr-stack"`, before the caller
/// touches that session's changeset. Shared by `add_planned_pr` today; future planned-PR
/// mutation RPCs (edit/delete) should call this too rather than re-checking inline.
pub(crate) fn require_pr_stack_orchestrator(session_dir: &std::path::Path) -> Result<(), Status> {
    let changeset = tddy_core::read_changeset(session_dir)
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    let recipe_name = changeset.recipe.as_deref().unwrap_or("");
    let is_pr_stack =
        tddy_workflow_recipes::recipe_resolve::resolve_workflow_recipe_from_cli_name(recipe_name)
            .map(|r| r.name() == "pr-stack")
            .unwrap_or(false);
    if !is_pr_stack {
        return Err(Status::failed_precondition(
            "session is not a pr-stack orchestrator",
        ));
    }
    Ok(())
}

/// The repoint target a client may act on: `Ok(None)` for "no target named", `Ok(Some(target))`
/// for an accepted one, `Err(reason)` for a target the daemon refuses.
///
/// `RepointPlannedPrRequest.target_base_branch` is applied by `repoint_planned_pr_node` as a
/// **retain** rule — the parents that own that branch stay and the rest are dropped — so a target
/// no parent owns *is* the instruction to detach the node onto the default branch. Validation is
/// therefore not politeness: a stale label, a typo, or a client that has drifted from the daemon's
/// view of the repo would each read as "detach this node" and silently rewrite the plan. An
/// accepted target must name either the resolved default branch or one of the node's parents'
/// branches; nothing else is a meaningful thing to be based onto.
///
/// An empty or whitespace-only target is not a rejection: it names no target at all and selects the
/// original drop-merged-parents rule (`None`).
///
/// The default branch is compared with the remote prefix stripped from both sides.
/// `tddy_core::resolve_default_integration_base_ref` returns a remote-tracking ref
/// (`<remote>/<branch>`), while a node's `branch` and a GitHub PR base are plain names, so the label
/// a client renders can legitimately carry either form. The remote is parsed off `default_branch`
/// (the segment before its first `/`) so a non-`origin` default is normalized correctly. The
/// accepted value returned is the caller's own trimmed input, not the normalized form, so the
/// recipe matches parent branches as recorded.
pub fn validate_repoint_target(
    target_base_branch: &str,
    default_branch: &str,
    parent_branches: &[&str],
) -> Result<Option<String>, String> {
    let target = target_base_branch.trim();
    if target.is_empty() {
        return Ok(None);
    }

    let remote = default_branch
        .split_once('/')
        .map(|(r, _)| r)
        .unwrap_or("origin");
    let names_default = tddy_core::worktree::local_branch_name_for_remote(target, remote)
        == tddy_core::worktree::local_branch_name_for_remote(default_branch, remote);
    let names_parent = parent_branches.contains(&target);

    if names_default || names_parent {
        Ok(Some(target.to_string()))
    } else {
        Err(format!(
            "target_base_branch '{target}' names neither the default branch '{default_branch}' nor any parent's branch"
        ))
    }
}
