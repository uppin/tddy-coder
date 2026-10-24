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

**ConnectionService** (local daemon) exposes **`ListWorktreesForProject`** and **`RemoveWorktree`** (see [daemon changelog](../daemon/changelog/)). Handlers use a shared **`WorktreeStatsCache`** rooted at **`projects_stats_cache_root()`**; **`refresh`** on the list RPC runs **`refresh_stats_for_project`** so ordinary polling can use **`refresh: false`** and stay on the cache-only path.

### Web UI (`tddy-web`)

In **daemon mode**, the shell **hamburger menu** (top left) includes **Worktrees**, navigating to **`/worktrees`**. The screen has the Sessions screen's layout: a **side panel** and a **main pane**.

- **Side panel.** A **project selector** (kept in `?project=`), a **Refresh** button that recalculates every worktree's size, and the project's worktrees listed **by name** (the directory name; the path and branch are in the row's tooltip) with their **disk usage** beside each. The list is **sorted by disk size, biggest first**; a worktree with no known size sorts last.
- **Main pane.** The selected worktree (the biggest until another is picked) shown in the same **Worktree pane** as the Session Inspector's Worktree tab (`SessionWorktreeTab`): Disk, Last calculated, Branch, Changed files, Refresh, Clear and a two-step Delete. There is no Restore, since the worktree has no session to be restored from. Deleting drops the worktree from the list and selects the next biggest.
- **No waiting.** The list opens on the worktrees and sizes remembered in the browser from the last visit (sizes shown in italics as stale). `StreamWorktreeStats` then supplies the daemon's current list with its **persisted** sizes, and any worktree never sized is calculated on the spot — a spinner on its row and a "Calculating disk usage… n of m" progress bar above the list. During a recalculation a row keeps its previous size beside the spinner until the new one arrives.

Worktree RPCs always hit the local daemon today, so there is no host selector.

## Operator references

| Topic | Detail |
|-------|--------|
| Stats baseline | Per-worktree **`git diff --numstat HEAD`** (working tree vs **`HEAD`**). |
| Cache location | **`TDDY_PROJECTS_STATS_ROOT`** or default **`~/.tddy/projects`** / **`{project_id}/worktree_stats.json`**. |
| Tests | **`cargo test -p tddy-daemon`** — **`worktrees_acceptance`**, **`worktrees_rpc`**; Cypress **`WorktreesScreen.cy.tsx`**, **`WorktreesScreenDiskUsage.cy.tsx`**, **`WorktreesAppPage.cy.tsx`**, **`WorktreesStreamingAcceptance.cy.tsx`**; unit **`src/lib/worktreeList.test.ts`**. |

## Related documentation

- [Web terminal / Connection screen](web-terminal.md) — daemon host selection and project-centric flows.
- [Local web development](local-web-dev.md) — **`./web-dev`** and **`/rpc`** proxy.
- Daemon package: [worktrees module](../../../packages/tddy-worktree-service/docs/worktrees.md).
