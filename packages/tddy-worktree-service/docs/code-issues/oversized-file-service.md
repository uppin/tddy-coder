# oversized-file: service.rs

**Location:** `packages/tddy-worktree-service/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **808 production lines** (2026-10-25, #752; 772 at 2026-10-18, #625; 777 at 2026-10-04, #571; `awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}'`; 752 at 2026-09-24) — originally **752 production lines** of 752 total (no `#[cfg(test)]`; 0 test lines) · budget 500 · **1.62× over**
**Thresholds breached:** length 772 > 500
**Restructure:** not designed
**Status:** Open — regressed 2026-10-25 (+36 from #752; 1.62× over) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 752 | first detection. The size predates #509 (753 at merge-base `4e7157d2`). #509 changed 1 line (0+/1−), dropping `.map(str::to_string)` from `authorize`'s `os_user_for_github` chain. It did not grow the file |
| 2026-10-04 | 777 | 752 at the base `c3567fde` → 777 after #571 (`#live-plan` 12/15): +25 lines, the session-ownership helper `session_dir_for` / `resolve_owned_session_dir`. Grown; the split is deferred because dependent #572 touches this file (the stack rule). Not claimed by any PR. The seam line ranges above predate this growth and must be re-derived |
| 2026-10-18 | 772 | 777 at the base → 772 after #625 (`#satellite` 3/13): −5 lines. The local `WorktreeRoomCloser` trait and `NoSessionRooms` became re-exports of the `SessionRooms` port. Improved, not clean; the split is still deferred (dependents of the `#keyring` stack touch this file) |
| 2026-10-25 | 808 | **regressed by #752** (`#start-parity` 3/9): 772 → 808 (+36) — `resolve_main_repo`'s body became the public `project_main_repo_on_host` (the project-home worktree tools resolve the same main repo), the `worktree_size_calculator()` accessor, and `stream_worktree_stats` reading its first frame through `worktrees::project_worktree_snapshot`. **Split deferred by developer instruction**: later `#start-parity` nodes (8/9 `disk-refresh`, 9/9) import `project_main_repo_on_host` from this module, so a move here breaks each of them on rebase. Seam ranges above need re-deriving |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | free helpers: `blocking_within`, `proto_worktree_size_status`, `worktree_row_from_diff`, `map_remove_worktree_error`, `map_clean_worktree_error` (L213–299) | ~85 |
| B | stats and size, from the `WorktreeService` impl: `stream_worktree_stats`, `calculate_worktree_size` (L610–751) | ~140 |
| C | file reading: `list_worktree_directory`, `read_worktree_file`, `stream_read_worktree_file` (L505–609) | ~105 |
| — | stays: struct, builders and auth/resolve helpers (L44–211), plus list/remove/clean/restore (L311–504) | — |

Seams B and C sit inside a trait impl. Their bodies move to free functions and the impl keeps delegating stubs. From this first reading, A+B+C take it to about 440 lines with stubs, and A+B alone leave it just over budget. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. `#keyring` 3/9 (#510) touches this file, so a split now would conflict with it.
