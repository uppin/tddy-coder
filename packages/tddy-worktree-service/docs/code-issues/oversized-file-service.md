# oversized-file: service.rs

**Location:** `packages/tddy-worktree-service/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **752 production lines** of 752 total (no `#[cfg(test)]`; 0 test lines) · budget 500 · **1.5× over**
**Thresholds breached:** length 752 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 752 | first detection. The size predates #509 (753 at merge-base `4e7157d2`). #509 changed 1 line (0+/1−), dropping `.map(str::to_string)` from `authorize`'s `os_user_for_github` chain. It did not grow the file |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | free helpers: `blocking_within`, `proto_worktree_size_status`, `worktree_row_from_diff`, `map_remove_worktree_error`, `map_clean_worktree_error` (L213–299) | ~85 |
| B | stats and size, from the `WorktreeService` impl: `stream_worktree_stats`, `calculate_worktree_size` (L610–751) | ~140 |
| C | file reading: `list_worktree_directory`, `read_worktree_file`, `stream_read_worktree_file` (L505–609) | ~105 |
| — | stays: `WorktreeRoomCloser` / `NoSessionRooms`, struct, builders and auth/resolve helpers (L44–211), plus list/remove/clean/restore (L311–504) | — |

Seams B and C sit inside a trait impl. Their bodies move to free functions and the impl keeps delegating stubs. From this first reading, A+B+C take it to about 440 lines with stubs, and A+B alone leave it just over budget. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. `#keyring` 3/9 (#510) touches this file, so a split now would conflict with it.
