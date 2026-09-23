//! The base-sync and worktree legs of a branch resolution: what `QueryBranch` and
//! `PullBaseIntoBranch` report about a branch beside its session, remote and PR.

/// Compare `branch` against `base_branch`, reading through the process-wide cache.
///
/// Resolving the two refs is a pair of `rev-parse`s and runs every time — it is what produces the
/// cache key, and it is also how a moved ref is noticed. Only the comparison itself, which runs
/// `git merge-tree`, is cached.
pub(crate) fn base_sync_through_cache(
    repo_root: &std::path::Path,
    branch: &str,
    base_branch: &str,
) -> Result<tddy_core::base_sync::BranchBaseSync, String> {
    let refs = tddy_core::base_sync::resolve_base_sync_refs(repo_root, branch, base_branch)?;
    let key = tddy_worktree_service::base_sync_cache::BaseSyncKey::new(repo_root, &refs);
    tddy_worktree_service::base_sync_cache::shared().get_or_probe(key, || {
        tddy_core::base_sync::compare_base_sync_refs(repo_root, &refs)
    })
}

/// A completed comparison on the wire. `base_branch` carries the ref that was actually compared —
/// not the one the caller asked for — because the counts are meaningless beside a ref they did not
/// come from (D28).
pub(crate) fn base_sync_view(
    sync: tddy_core::base_sync::BranchBaseSync,
) -> tddy_service::proto::pr_stack::BranchBaseSync {
    tddy_service::proto::pr_stack::BranchBaseSync {
        base_branch: sync.base_ref.clone(),
        behind_count: sync.behind_count,
        ahead_count: sync.ahead_count,
        has_conflicts: sync.has_conflicts,
        conflicted_paths: sync.conflicted_paths,
        unavailable: false,
        unavailable_reason: String::new(),
        base_ref: sync.base_ref,
        head_ref: sync.head_ref,
    }
}

/// A comparison the daemon could not make: *unavailable* with an operator-facing reason, never a
/// zeroed success. A failed comparison reads identically to a healthy one on every other field, so
/// this discriminator is the only thing standing between "could not tell" and "clean" (D27).
pub(crate) fn base_sync_unavailable(
    base_branch: &str,
    reason: &str,
) -> tddy_service::proto::pr_stack::BranchBaseSync {
    tddy_service::proto::pr_stack::BranchBaseSync {
        base_branch: base_branch.to_string(),
        unavailable: true,
        unavailable_reason: reason.to_string(),
        ..Default::default()
    }
}

/// The `worktree` leg of a `BranchResolution`: the on-disk worktree checked out for `branch`, and
/// whether it holds outstanding work.
///
/// Two git subprocesses — a `git worktree list` walk and a `git status --porcelain` — so every caller
/// runs this on the blocking pool, never on a runtime thread.
pub(crate) fn worktree_leg(
    repo_root: Option<&std::path::Path>,
    branch: &str,
) -> tddy_service::proto::pr_stack::BranchWorktree {
    use tddy_service::proto::pr_stack::BranchWorktree;

    let Some(path) =
        repo_root.and_then(|root| tddy_core::worktree::worktree_path_for_branch(root, branch))
    else {
        return BranchWorktree::default();
    };
    let dirty_paths = worktree_dirty_paths(&path);
    BranchWorktree {
        exists: true,
        path: path.to_string_lossy().into_owned(),
        dirty: !dirty_paths.is_empty(),
        dirty_paths,
    }
}

/// The tracked paths with outstanding changes in a worktree — empty for a clean one, and empty for
/// a path git cannot read at all, which is the same thing as far as offering a pull goes.
///
/// Untracked files are deliberately excluded: git refuses loudly rather than clobbering one, and
/// counting them would leave the pull control permanently blocked in any worktree an agent works in.
fn worktree_dirty_paths(worktree: &std::path::Path) -> Vec<String> {
    tddy_workflow_recipes::orchestrate_pr_stack::worktree_is_clean(worktree).unwrap_or_else(|e| {
        log::warn!(
            "QueryBranch: could not read the state of the worktree at {}: {e}",
            worktree.display()
        );
        Vec::new()
    })
}
