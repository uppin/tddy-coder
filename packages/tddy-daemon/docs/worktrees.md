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
| **`RemoveWorktreeError`** | **`GitFailed`**, **`NotListed`**, **`CannotRemovePrimary`**, **`Io`**. |
| **`remove_worktree_under_repo`** | Validates membership via **`git worktree list`**, blocks primary row, runs **`git worktree remove`**. |

## Persistence layout

```
{TDDY_PROJECTS_STATS_ROOT or ~/.tddy/projects}/{sanitized_project_id}/worktree_stats.json
```

**`sanitized_project_id`** replaces **`/`**, **`\`**, **`:`** with **`_`**.

## Logging

Uses the **`log`** crate (**`debug!`**, **`info!`**, **`warn!`**) for parse, cache, and git subprocess outcomes.

## Tests

- Unit tests in **`src/worktrees.rs`**: parser fixtures; path policy.
- Integration tests in **`tests/worktrees_acceptance.rs`**: cache counter semantics (requires **`git`**).

## Feature documentation

- [Web Worktrees manager](../../../docs/ft/web/worktrees.md)
