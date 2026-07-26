/**
 * Branch-name vocabulary shared by the session-creation form and the PR-Stack view.
 *
 * The daemon speaks two forms of the same branch. `ListProjectBranches` lists **remote-tracking**
 * refs (`origin/feature/x` — `list_recent_remote_branches` reads `refs/remotes/origin`), so every
 * option in a branch picker carries that prefix. Everything else — a stack node's `branch`, a
 * session's `branch`, the branch a spawn is linked by — names the **local** branch.
 */

/**
 * The local branch name behind a branch reference: `origin/feature/x` → `feature/x`, and any name
 * that is already local unchanged.
 *
 * Exactly one leading `origin/` is stripped: a repository may legitimately hold a local branch called
 * `origin/legacy`, and stripping repeatedly would rename it. Mirrors
 * `tddy_core::worktree::local_branch_name`, which the daemon applies to the same value on arrival.
 */
export function localBranchName(reference: string): string {
  const trimmed = reference.trim();
  return trimmed.startsWith("origin/") ? trimmed.slice("origin/".length) : trimmed;
}
