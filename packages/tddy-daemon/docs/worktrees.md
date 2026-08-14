# Worktrees module (`tddy_daemon::worktrees`)

## Role

Library helpers for the Worktrees manager feature: parse **`git worktree list`**, persist per-project worktree statistics, validate client-supplied paths against a repository root (lexical normalization), and remove secondary worktrees via **`git worktree remove`**.

## Public API (summary)

| Item | Role |
|------|------|
| **`WorktreeListRow`** | Parsed row: **`path`**, **`branch_label`**, optional **`lock_path`**. |
| **`WorktreeStatSnapshot`** | Serializable snapshot for cache files: disk bytes, diff stats, timestamps, **`stale`**. |
| **`WorktreePathError`** | **`OutsideRepoRoot`** when lexical resolution leaves the repo prefix. |
| **`parse_git_worktree_list`** | Parses non-porcelain **`git worktree list`** stdout. |
| **`projects_stats_cache_root`** | Root directory; honors **`TDDY_PROJECTS_STATS_ROOT`**. |
| **`validate_worktree_path_within_repo_root`** | Lexical **`..`** / absolute resolution; no filesystem canonicalize. |
| **`WorktreeStatsCache`** | **`new`**, **`refresh_stats_for_project`**, **`list_cached_stats`**, **`invalidate_project`**. Test-only atomic **`test_git_diff_invocations`** counts refresh-side diff/stat work; list path does not re-run diff. |
| **`WorktreeNumstat`** | Parsed `git diff --numstat HEAD`: **`paths`**, **`changed_files`**, **`lines_added`**, **`lines_removed`**. |
| **`git_diff_numstat`** | Runs and parses that diff. Shared with `session_room`, so a session room and the Worktrees screen can never quote different totals for one checkout. Paths arrive as git presents them — C-quoted for non-ASCII, `{old => new}` for renames — so they are display-only. |
| **`RemoveWorktreeError`** | **`GitFailed`**, **`NotListed`**, **`CannotRemovePrimary`**, **`Io`**. |
| **`remove_worktree_under_repo`** | Validates membership via **`git worktree list`**, blocks primary row, runs **`git worktree remove`**. |
| **`CleanWorktreeError`** | **`GitFailed`**, **`NotListed`**, **`CannotCleanPrimary`**, **`Io`**. |
| **`clean_worktree_under_repo`** | Validates membership via **`git worktree list`**, blocks primary row, runs **`git clean -fdx`** in the worktree (reclaims disk without removing it). Mirrors **`remove_worktree_under_repo`**. |
| **`WorktreeSizeStatus`** | Size lifecycle: **`None`** (never calculated), **`Calculating`**, **`Cached`**. |
| **`WorktreeSizeState`** / **`WorktreeSizeUpdate`** | Current state / a published transition (`status`, `disk_bytes`, `calculated_at_unix_ms`; the update also carries `path`). |
| **`WorktreeSizeCalculator`** | **`new(root, permits)`** / **`with_sizer(root, permits, sizer)`**; **`state`**, **`enqueue`**, **`subscribe`**, **`snapshot`**. Lazy, semaphore-bounded per-worktree disk-size lifecycle (see below). |

## Lazy per-worktree disk size (`WorktreeSizeCalculator`)

The expensive directory-size walk is computed **lazily, per worktree, and centrally rate-limited** rather than in the eager project-wide sweep of `WorktreeStatsCache`:

- A single **daemon-global `tokio::sync::Semaphore`** (default **2** permits) bounds concurrent walks across all projects/worktrees. The permit is acquired **inside** each spawned walk, so all enqueues start immediately and only `permits` walks run at once.
- **`enqueue`** marks the worktree `Calculating` and broadcasts it, de-duplicates an already-in-flight walk, runs the injected sizer (prod: `directory_size_bytes_best_effort`) under `spawn_blocking`, then marks `Cached` (bytes + `calculated_at_unix_ms`), broadcasts, and persists. No lock is held across an `.await`.
- **`subscribe(project)`** returns a `tokio::sync::broadcast::Receiver<WorktreeSizeUpdate>`; **`snapshot(project)`** returns every known worktree's state (the stream's first frame). **`state`** reads memory, lazily falling back to the persisted file so a fresh calculator reports `Cached` without re-walking.
- The `git diff` summary (changed files, ±lines) is unchanged — only the size walk is lazy/status-tracked.

`ConnectionService` wires an `Arc<WorktreeSizeCalculator>` and exposes it over the streaming `StreamWorktreeStats` (snapshot + per-worktree `Calculating→Cached` increments, via `MpscWorktreeStatsStream`) and the membership-gated unary `CalculateWorktreeSize`; `ListWorktreesForProject` overlays the size status/timestamp while retaining its eager `disk_bytes` walk as a cache fallback.

## Persistence layout

```
{TDDY_PROJECTS_STATS_ROOT or ~/.tddy/projects}/{sanitized_project_id}/worktree_stats.json   # eager stats cache
{TDDY_PROJECTS_STATS_ROOT or ~/.tddy/projects}/{sanitized_project_id}/worktree_sizes.json   # lazy per-worktree sizes
```

**`sanitized_project_id`** replaces **`/`**, **`\`**, **`:`** with **`_`**.

## Logging

Uses the **`log`** crate (**`debug!`**, **`info!`**, **`warn!`**) for parse, cache, and git subprocess outcomes.

## Tests

- Unit tests in **`src/worktrees.rs`**: parser fixtures; path policy.
- Integration tests in **`tests/worktrees_acceptance.rs`**: cache counter semantics (requires **`git`**).
- **`tests/worktree_size_calculator_acceptance.rs`**: `WorktreeSizeCalculator` status model, `None→Calculating→Cached` transitions, semaphore-bounded concurrency (=2), persistence-without-recompute, single-worktree isolation, de-dup, snapshot.
- **`tests/stream_worktree_stats_rpc.rs`**: `StreamWorktreeStats` snapshot + increment + `CalculateWorktreeSize` membership gate.

## Feature documentation

- [Web Worktrees manager](../../../docs/ft/web/worktrees.md)
