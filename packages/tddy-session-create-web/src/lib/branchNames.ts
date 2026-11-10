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

/**
 * The remote-tracking ref for a local branch name: prepends `<remote>/` when the name does not already
 * carry that prefix, and leaves a name that is already remote-tracking unchanged. The inverse of
 * [`localBranchName`].
 *
 * The daemon's `selected_integration_base_ref` is a remote-tracking ref (`<remote>/<branch>`, e.g.
 * `origin/feature/x`), while the rest of the domain — a stack node's `branch`, a session's `branch` —
 * names the local branch. This helper bridges the two at the Start-session dialog's submit seam so the
 * daemon receives the form it validates and fetches (`git fetch <remote> <branch>`), instead of a bare
 * local name whose first path segment (`feature/...`) it would mistake for a remote.
 *
 * Idempotent: `remoteTrackingName("origin/master", "origin")` returns `"origin/master"`, so it is safe
 * to apply to a value that may already be remote-tracking (e.g. `ProjectEntry.main_branch_ref`).
 *
 * @param branch  The local branch name (e.g. `feature/x`) or an already-remote-tracking ref
 *                (e.g. `origin/master`). An empty/whitespace string is returned unchanged.
 * @param remote  The project's resolved default remote (`origin`, `upstream`, ...). Defaults to
 *                `"origin"` for callers that have not threaded the resolved remote — the legacy
 *                behavior — but new code should pass the value from
 *                `ProjectEntry.defaultRemote` / `ListProjectBranchesResponse.defaultRemote`.
 */
export function remoteTrackingName(branch: string, remote = "origin"): string {
  const trimmed = branch.trim();
  if (trimmed === "") return "";
  const prefix = `${remote}/`;
  return trimmed.startsWith(prefix) ? trimmed : `${prefix}${trimmed}`;
}
