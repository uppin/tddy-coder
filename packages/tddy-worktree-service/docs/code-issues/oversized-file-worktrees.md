# oversized-file: worktrees.rs

**Location:** `packages/tddy-worktree-service/src/worktrees.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 file-length gate on #752 (`#start-parity` 3/9), production lines counted with `awk '/^[[:space:]]*#\[cfg\(.*test[),]/ {exit} {n++} END {print n+0}'`
**Metrics:** **1003 production lines** · budget 500 · **2.0× over**
**Thresholds breached:** length 1003 > 500
**Restructure:** not designed
**Status:** Open — split deferred: later `#start-parity` nodes (8/9 `disk-refresh`, 9/9 `spawn-conversation`) import `project_worktree_snapshot` from this module, so a split mid-stack breaks each of them on rebase. Follow-up after the stack lands — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 1003 | first detection. 975 at the base (the `#start-parity` 2/9 tip) — the size predates this PR. #752 added 28: `DAEMON_SIZE_WALK_PERMITS`, `WorktreeSnapshotRow` and `project_worktree_snapshot` (the snapshot `StreamWorktreeStats` opens with and the project-home `ListWorktrees` reads). **Split deferred by developer instruction** (stack-shared) |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | `git worktree list` parsing and the stats cache: `WorktreeListRow`, `WorktreeStatSnapshot`, `parse_git_worktree_list*`, `WorktreeStatsCache` and its file helpers (L17–365) | ~350 |
| B | the size calculator: `WorktreeSizeState`, `WorktreeSizeUpdate`, `WorktreeSizeCalculator`, the sizes file and `directory_size_bytes_best_effort` (L367–745) | ~380 |
| C | diff summary and the snapshot: `WorktreeNumstat`, `git_diff_numstat`, `WorktreeDiffRow`, `list_worktree_diff_rows`, `WorktreeSnapshotRow`, `project_worktree_snapshot` (L747–878) | ~130 |
| — | stays: path validation and the remove/clean operations | — |

Any two of A–C bring it under budget; keep `pub use` re-exports so `worktrees::…` paths survive. Prove
the seams with `restructure check --deep` before applying them.
