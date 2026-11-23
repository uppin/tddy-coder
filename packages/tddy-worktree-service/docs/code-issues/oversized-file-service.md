# oversized-file: service.rs

**Location:** `packages/tddy-worktree-service/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **831 production lines** (2026-11-21, #975; 883 at 2026-11-21, #970; 808 at 2026-10-25, #752; 772 at 2026-10-18, #625; 777 at 2026-10-04, #571; `awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}'`; 752 at 2026-09-24) — originally **752 production lines** of 752 total (no `#[cfg(test)]`; 0 test lines) · budget 500 · **1.66× over**
**Thresholds breached:** length 831 > 500
**Restructure:** not designed
**Status:** Open — narrowed 2026-11-23 (#1022: 897 → 679, seam W moved the inherent impl to `service/host.rs`); the remainder is the `impl WorktreeService` trait impl (seams B, C), which the engine cannot cut — no hand move consented (D2). **TODO:** re-measure after the engine can extract from trait impls (`docs/dev/todo/2026-11-14-restructure-engine-no-extract-for-guarded-ranges.md`). Earlier: partially fixed 2026-11-21 (#975: 883 → 831); engine defect: `docs/dev/todo/2026-11-21-restructure-engine-refuses-the-target-parity-splits.md`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 752 | first detection. The size predates #509 (753 at merge-base `4e7157d2`). #509 changed 1 line (0+/1−), dropping `.map(str::to_string)` from `authorize`'s `os_user_for_github` chain. It did not grow the file |
| 2026-10-04 | 777 | 752 at the base `c3567fde` → 777 after #571 (`#live-plan` 12/15): +25 lines, the session-ownership helper `session_dir_for` / `resolve_owned_session_dir`. Grown; the split is deferred because dependent #572 touches this file (the stack rule). Not claimed by any PR. The seam line ranges above predate this growth and must be re-derived |
| 2026-10-18 | 772 | 777 at the base → 772 after #625 (`#satellite` 3/13): −5 lines. The local `WorktreeRoomCloser` trait and `NoSessionRooms` became re-exports of the `SessionRooms` port. Improved, not clean; the split is still deferred (dependents of the `#keyring` stack touch this file) |
| 2026-10-25 | 808 | **regressed by #752** (`#start-parity` 3/9): 772 → 808 (+36) — `resolve_main_repo`'s body became the public `project_main_repo_on_host` (the project-home worktree tools resolve the same main repo), the `worktree_size_calculator()` accessor, and `stream_worktree_stats` reading its first frame through `worktrees::project_worktree_snapshot`. **Split deferred by developer instruction**: later `#start-parity` nodes (8/9 `disk-refresh`, 9/9) import `project_main_repo_on_host` from this module, so a move here breaks each of them on rebase. Seam ranges above need re-deriving |
| 2026-11-21 | 883 | **regressed by #970** (`#target-parity` 2/7): 808 → 883 (+75) — the `target_shells` field and `with_target_shells` builder, `shell_of_target` / `target_session_of`, and the target dispatch at the head of `remove_worktree`, `clean_worktree` and `restore_session_worktree`; the work itself is in the new `target_worktrees.rs`. **Split not done here**: the file is stack-shared with `#target-parity` 7/7 (#975), so it is recorded, not split, under the stack rule |
| 2026-11-21 | 831 | **improved by #975** (`#target-parity` 7/7): 883 → 831 (−52). Seam A — `blocking_within`, `proto_worktree_size_status`, `worktree_row_from_diff`, `map_remove_worktree_error`, `map_clean_worktree_error` — moved to `service_helpers.rs`; n7's target-row code lives in `target_worktree_stats.rs`, with two call sites, `sessions_dir_of` and one bus accessor here |

| 2026-11-21 | 831 | unchanged by the `#target-parity` sweep: **split attempted and refused by the engine** — see below |
| 2026-11-22 | 897 | **regressed by #1005** (`#workqueue` 4/14): 831 → 897 (+66). This PR's four inspector RPCs had taken the file to 986; their bodies (`session_sync` and the PR stream, 115 lines) now live in the child module `service/inspector.rs`. What stays here: the four delegating trait methods and the stream's associated type, the `pull_requests` field and `attach_pull_requests`, and imports. The pre-existing seams are untouched, because they wait on the engine (status above) |

| 2026-11-23 | 679 | **narrowed by the `#workqueue` sweep** (#1022): 897 → 679 (−218). Seam W — the whole inherent `impl WorktreeServiceImpl` — moved to `service/host.rs` (257 lines) with `extract_module` + `to_file`, which the engine accepted (`check --deep` clean, apply compiled). Still **over budget**: the `impl WorktreeService` trait impl (seams B and C above) stays, and the engine still cannot cut it |

## Engine refusal (2026-11-21)

Seam C's first method, `list_worktree_directory`, as the record prescribes: `read_fields_through`
(`expr: self`, `name: service`) then `extract_method` over the same item-relative range of the body, one
`group`, with the warm index daemon:

```text
$ ./dev target/debug/tddy-tools restructure check tmp/restructure/service-c1.jsonl --deep
   note: read_fields_through: rebound every self (2 sites) through `service`
1: group `c1` is refused whole, because a member of it is: op 1: this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the given range (it offered: Extract into variable)
Error: 1 finding(s) — see above. Nothing was written.
```

A second item anchor shifted past the inserted `let` cannot be written: item anchors resolve against
the tree the run opens on, and a range without a fingerprint is refused as malformed.

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| B | stats and size, from the `WorktreeService` impl: `stream_worktree_stats`, `calculate_worktree_size` (L610–751) | ~140 |
| C | file reading: `list_worktree_directory`, `read_worktree_file`, `stream_read_worktree_file` (L505–609) | ~105 |
| — | stays: struct, builders and auth/resolve helpers (L44–211), plus list/remove/clean/restore (L311–504) | — |

Seams B and C sit inside a trait impl. Their bodies move to free functions and the impl keeps delegating stubs. Seam A is done (#975). **B+C alone land near 590 lines, not 440** (re-measured 2026-11-21): the five trait-method signatures, their doc comments and the delegating calls stay. A third seam is needed — the inherent resolve helpers (`target_session_of`, `resolve_main_repo`, and the free `project_main_repo_on_host`, about 110 lines) through `move_impl_members` / `move_item` into a child module of `service`. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. `#keyring` 3/9 (#510) touches this file, so a split now would conflict with it.
