# Web Worktrees manager

## Purpose

Operators inspect Git worktrees per registered project: paths, branches, on-disk size, changed-file counts, and added/removed line counts, with a path to delete secondary worktrees. Expensive statistics belong on a refresh cadence and on-disk cache so dashboard polling does not multiply `git` or filesystem work.

## Implementation scope

### Daemon library (`tddy-daemon`)

The **`worktrees`** module provides:

- Parsing of default **`git worktree list`** output into structured rows; detached HEAD lines use the branch label **`(detached)`**.
- Lexical path containment checks relative to a repository root for callers that need prefix policy (for example RPC path validation).
- A **`WorktreeStatsCache`** that stores JSON snapshots under **`TDDY_PROJECTS_STATS_ROOT`**, defaulting to **`~/.tddy/projects`** when **`HOME`** is set. Integration tests set **`TDDY_PROJECTS_STATS_ROOT`** to a temporary directory.
- **`refresh_stats_for_project`**: runs **`git worktree list`** from the project main repo, computes per-worktree directory size and **`git diff --numstat HEAD`** aggregates (changed files, lines added/removed), writes **`{cache_root}/{sanitized_project_id}/worktree_stats.json`**.
- **`list_cached_stats`**: reads the last persisted snapshot from disk without running diff on the hot path.
- **`remove_worktree_under_repo`**: requires the target path to appear in **`git worktree list`**, refuses removal of the primary (first-listed) worktree, runs **`git worktree remove`**. Secondary worktrees may live outside the main repo directory (sibling paths); membership in Git’s list is the gate.

**ConnectionService** exposes no worktree-specific RPCs yet; call sites are expected to use this library from future handlers.

### Web UI (`tddy-web`)

**`WorktreesScreen`** renders a table with path, branch, size label, changed files, +/- lines, and a two-step **Delete** → **Confirm delete** action. Component tests mount the screen with mocked rows; production wiring to **`ListEligibleDaemons`**, project lists, and daemon RPCs is out of scope for the current library-first milestone.

## Operator references

| Topic | Detail |
|-------|--------|
| Stats baseline | Per-worktree **`git diff --numstat HEAD`** (working tree vs **`HEAD`**). |
| Cache location | **`TDDY_PROJECTS_STATS_ROOT`** or default **`~/.tddy/projects`** / **`{project_id}/worktree_stats.json`**. |
| Tests | **`cargo test -p tddy-daemon --no-fail-fast`** (includes **`worktrees_rpc`**); Cypress **`cypress/component/WorktreesScreen.cy.tsx`**. |

## Related documentation

- [Web terminal / Connection screen](web-terminal.md) — daemon host selection and project-centric flows.
- [Local web development](local-web-dev.md) — **`./web-dev`** and **`/rpc`** proxy.
- Daemon package: [worktrees module](../../../packages/tddy-daemon/docs/worktrees.md).
