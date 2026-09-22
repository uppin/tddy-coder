//! Taking a base branch's commits into a node's branch, inside that node's own worktree.

use std::path::Path;

/// How a node's branch takes its base's commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BaseSyncStrategy {
    /// Add a merge commit. The default: it rewrites no history, needs no force-push, and disturbs
    /// none of the review anchors on the node's open pull request.
    #[default]
    Merge,
    /// Replay the branch's own commits on top of the base. Rewrites history, so it force-pushes
    /// with a lease — the right trade only when the operator chooses it.
    Rebase,
}

impl BaseSyncStrategy {
    /// The wire vocabulary: `"merge"` (also what an unnamed strategy means) or `"rebase"`.
    #[must_use]
    pub fn from_wire(strategy: &str) -> Self {
        match strategy.trim() {
            "rebase" => Self::Rebase,
            _ => Self::Merge,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }
}

/// The `dirty_worktree_action` that commits and pushes outstanding tracked changes before pulling.
/// Anything else — including the empty default — refuses a dirty worktree instead.
const COMMIT_DIRTY_WORKTREE: &str = "commit";

/// What a pull did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullBaseReport {
    /// `"merge"` or `"rebase"` — the strategy that actually ran.
    pub strategy: &'static str,
    /// False when the branch already contained every commit on its base, in which case nothing was
    /// committed and nothing was pushed.
    pub changed: bool,
    /// The branch's tip after the pull.
    pub head_sha: String,
    pub pushed: bool,
    /// Why the push failed, when it did. The local work landed regardless — see
    /// [`pull_base_into_node_branch`].
    pub push_error: Option<String>,
}

/// Take a base branch's commits into a node's branch, inside that node's **own worktree**, and push
/// the result.
///
/// This is the operator's "stay where I am and take what the base has", distinct from a repoint
/// (which answers "this node belongs somewhere else now" by dropping parent edges).
///
/// The order of the checks is the safety design, because the button may be pressed while a child
/// session's agent is mid-turn in the worktree being touched:
///
/// 1. A node that does not exist, owns no branch, or was given no base is refused outright.
/// 2. A branch with no worktree is refused **by name**. It is deliberately not checked out in the
///    main repository instead: `git_ops::rebase_onto` does that, and it is a latent clobbering
///    hazard this does not extend. The worktree that is found must have this very branch checked
///    out — the resolver also answers with one that merely shares the branch's tip commit, which is
///    fine for displaying an indicator and would silently move a sibling's branch here.
/// 3. The worktree is checked for outstanding tracked changes **before the fetch** — before
///    anything touches git state at all. A refusal therefore leaves not just the tree but the
///    repository's remote-tracking refs exactly as they were. `dirty_worktree_action = "commit"`
///    commits those changes under `commit_message` and pushes them, then continues; anything else
///    refuses and names the paths. Untracked files are not outstanding work and never block.
/// 4. Only the base ref is fetched — never `git fetch <remote>` wholesale, which would also refresh
///    the remote-tracking ref a rebase's `--force-with-lease` is taken against and so turn "somebody
///    else pushed while you were rebasing" from a refused push into a clobbering one.
/// 5. A conflict aborts and is refused, naming the paths (PRD D33). The node is **not** stamped
///    `has-conflicts`: conflicts are a live fact on every poll now, so a persisted stamp can only go
///    stale, and clearing one risks stomping an agent's own `source: "override"`.
/// 6. A failed **push** is reported as `Ok` with `pushed = false` and a `push_error`, not as an
///    error (PRD D32). The local merge or rebase landed; rolling it back would be strictly worse
///    than saying so.
pub fn pull_base_into_node_branch(
    session_dir: &Path,
    repo_root: &Path,
    node_id: &str,
    base_branch: &str,
    strategy: BaseSyncStrategy,
    dirty_worktree_action: &str,
    commit_message: &str,
) -> Result<PullBaseReport, String> {
    use crate::git_ops;
    const OP: &str = "pull_base_into_node_branch";

    let base_branch = base_branch.trim();
    if base_branch.is_empty() {
        return Err(format!(
            "{OP}: node '{node_id}' was given no base branch to pull from"
        ));
    }

    let stack = tddy_core::changeset::read_changeset(session_dir)
        .map_err(|e| format!("{OP}: failed to read changeset: {e}"))?
        .stack
        .unwrap_or_default();
    let node = stack
        .node(node_id)
        .ok_or_else(|| format!("{OP}: node '{node_id}' not found"))?;
    let branch = node.branch.clone().ok_or_else(|| {
        format!(
            "{OP}: node '{node_id}' owns no branch yet, so there is nothing to pull '{base_branch}' \
             into — start its session first"
        )
    })?;

    let remote =
        tddy_git::detect_default_remote_name(repo_root).unwrap_or_else(|| "origin".to_string());
    let base_branch = tddy_git::local_branch_name_for_remote(base_branch, &remote);

    // The pull runs in the node's own worktree. Checking the branch out anywhere else would move a
    // checkout the operator did not ask about.
    let worktree = tddy_git::worktree_path_for_branch(repo_root, &branch).ok_or_else(|| {
        format!(
            "{OP}: branch '{branch}' has no worktree checked out, so there is nowhere to pull \
                 '{base_branch}' into — start or resume the session that owns it first"
        )
    })?;
    // `worktree_path_for_branch` also answers with a worktree that merely *shares* the branch's tip
    // commit — a sibling node branched off this one and not yet committed resolves to exactly that.
    // Displaying such a worktree is harmless; merging into it would land this node's base on the
    // sibling's branch and then push this branch, which never moved, so git says "Everything
    // up-to-date" and the operator is told a pull happened that did not.
    let checked_out = tddy_git::checked_out_branch_name(&worktree).map_err(|e| {
        format!(
            "{OP}: could not read what '{}' has checked out: {e}",
            worktree.display()
        )
    })?;
    if checked_out.as_deref() != Some(branch.as_str()) {
        return Err(format!(
            "{OP}: the worktree found for '{branch}' ({}) has '{}' checked out, not '{branch}' — \
             pulling '{base_branch}' there would move the wrong branch; start or resume the session \
             that owns '{branch}' first",
            worktree.display(),
            checked_out.as_deref().unwrap_or("a detached HEAD")
        ));
    }

    // Before the fetch, before anything touches git state: a refusal here leaves the repository
    // byte-for-byte as it was found, remote-tracking refs included.
    let outstanding = git_ops::worktree_is_clean(&worktree)
        .map_err(|e| format!("{OP}: could not read the state of '{branch}': {e}"))?;
    if !outstanding.is_empty() {
        if dirty_worktree_action.trim() != COMMIT_DIRTY_WORKTREE {
            return Err(format!(
                "{OP}: the worktree for '{branch}' has uncommitted changes to {} — commit them, or \
                 re-run asking for them to be committed first",
                outstanding.join(", ")
            ));
        }
        commit_outstanding_work(OP, &worktree, &remote, &branch, commit_message)?;
    }

    git_ops::fetch_ref(&worktree, &remote, base_branch)
        .map_err(|e| format!("{OP}: could not fetch '{base_branch}' from '{remote}': {e}"))?;
    let base_ref = format!("{remote}/{base_branch}");
    if git_ops::ref_sha(&worktree, &base_ref).is_none() {
        return Err(format!(
            "{OP}: '{base_ref}' names no commit after fetching from '{remote}', so '{branch}' has \
             no base to take"
        ));
    }

    // Captured before the rewrite: the lease is the promise that the remote still holds what this
    // clone last saw, so a concurrent push aborts the force-push rather than being clobbered by it.
    //
    // `None` — no remote-tracking ref at all — is deliberately not an empty lease. Git reads
    // `--force-with-lease=<branch>:` as "the branch must be *absent* on the remote", so a branch that
    // is on the remote but has never been fetched into this clone would have every rebase push
    // refused with an opaque "stale info". There is nothing to take a lease against here, so the push
    // below falls back to a plain one, which the remote refuses unless it fast-forwards: a branch
    // this clone has never seen therefore either gets created or gets an honest non-fast-forward
    // rejection, and neither can clobber anyone's work.
    let lease_sha = match strategy {
        BaseSyncStrategy::Rebase => {
            git_ops::ref_sha(&worktree, &format!("refs/remotes/{remote}/{branch}"))
        }
        BaseSyncStrategy::Merge => None,
    };

    let outcome = match strategy {
        BaseSyncStrategy::Merge => git_ops::merge_ref_into_worktree(&worktree, &base_ref),
        BaseSyncStrategy::Rebase => git_ops::rebase_branch_onto_ref(&worktree, &base_ref),
    }
    .map_err(|e| {
        format!(
            "{OP}: could not {} '{base_ref}' into '{branch}': {e}",
            strategy.name()
        )
    })?;

    let head_sha = match outcome {
        // Already aborted by the primitive, so the worktree is exactly where it started.
        git_ops::SyncOutcome::Conflicted(paths) => {
            return Err(format!(
                "{OP}: '{branch}' conflicts with '{base_ref}' in {} — resolve them in the worktree \
                 (the {} was aborted and nothing was left half-applied)",
                paths.join(", "),
                strategy.name()
            ))
        }
        git_ops::SyncOutcome::AlreadyUpToDate => {
            return Ok(PullBaseReport {
                strategy: strategy.name(),
                changed: false,
                head_sha: git_ops::head_sha(&worktree)
                    .map_err(|e| format!("{OP}: could not read the tip of '{branch}': {e}"))?,
                pushed: false,
                push_error: None,
            })
        }
        git_ops::SyncOutcome::Applied(sha) => sha,
    };

    // A push that fails is reported, never rolled back: the merge or rebase is real local work, and
    // undoing it would be strictly worse than saying the remote does not have it yet.
    let push = match lease_sha {
        Some(lease_sha) => git_ops::force_push_with_lease(&worktree, &remote, &branch, &lease_sha),
        None => git_ops::push_branch(&worktree, &remote, &branch),
    };
    let push_error = push.err().map(|e| e.to_string());

    Ok(PullBaseReport {
        strategy: strategy.name(),
        changed: true,
        head_sha,
        pushed: push_error.is_none(),
        push_error,
    })
}

/// Commit the worktree's outstanding tracked changes and push them, so the pull starts from a clean
/// tree without any of the operator's work having been stashed, discarded or merged into.
fn commit_outstanding_work(
    op: &str,
    worktree: &Path,
    remote: &str,
    branch: &str,
    commit_message: &str,
) -> Result<(), String> {
    let commit_message = commit_message.trim();
    if commit_message.is_empty() {
        return Err(format!(
            "{op}: committing the outstanding changes in '{branch}' first needs a commit message"
        ));
    }
    crate::git_ops::commit_all_tracked(worktree, commit_message).map_err(|e| {
        format!("{op}: could not commit the outstanding changes in '{branch}': {e}")
    })?;
    crate::git_ops::push_branch(worktree, remote, branch).map_err(|e| {
        format!(
            "{op}: the outstanding changes in '{branch}' were committed but could not be pushed: \
             {e}"
        )
    })
}
