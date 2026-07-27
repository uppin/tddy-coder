/**
 * Branch-name vocabulary shared by the session-creation form and the PR-Stack view.
 *
 * The daemon speaks two forms of the same branch. `ListProjectBranches` lists **remote-tracking**
 * refs (`<remote>/feature/x` — `list_recent_remote_branches` reads `refs/remotes/<remote>`), so every
 * option in a branch picker carries that prefix. Everything else — a stack node's `branch`, a
 * session's `branch`, the branch a spawn is linked by — names the **local** branch.
 *
 * The remote is **not** assumed to be `origin`: the daemon resolves the project's default remote
 * (main worktree upstream → project config → `origin` last resort) and exposes it via
 * `ListProjectBranchesResponse.defaultRemote` / `ProjectEntry.defaultRemote`. Callers that have the
 * resolved remote should pass it to [`localBranchName`] so a non-`origin` prefix is stripped
 * correctly; the no-arg form keeps `origin` only as the legacy fallback.
 */

/**
 * The local branch name behind a branch reference: strips one leading `<remote>/` when present, and
 * leaves any name that is already local (or carries a different remote prefix) unchanged.
 *
 * Exactly one leading `<remote>/` is stripped: a repository may legitimately hold a local branch
 * called `<remote>/legacy`, and stripping repeatedly would rename it. Mirrors
 * `tddy_core::worktree::local_branch_name_for_remote`, which the daemon applies to the same value on
 * arrival.
 *
 * @param reference  The branch reference to normalize (e.g. `upstream/feature/x`).
 * @param remote     The project's resolved default remote (`origin`, `upstream`, ...). Defaults to
 *                   `"origin"` for callers that have not threaded the resolved remote — the legacy
 *                   behavior — but new code should pass the value from
 *                   `ListProjectBranchesResponse.defaultRemote` / `ProjectEntry.defaultRemote`.
 */
export function localBranchName(reference: string, remote = "origin"): string {
  const trimmed = reference.trim();
  const prefix = `${remote}/`;
  return trimmed.startsWith(prefix) ? trimmed.slice(prefix.length) : trimmed;
}
