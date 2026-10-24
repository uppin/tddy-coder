# oversized-file: project/coordinate_handlers.rs

**Location:** `packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **564 production lines** (no `#[cfg(test)]`; 2026-10-24) · budget 500 · **1.3× over** (528 at detection)
**Thresholds breached:** length 650 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed** — regressed 2026-10-04 (+122, #512); split deferred with the developer's consent, see [`docs/dev/todo/2026-10-04-keyring-assignments-grew-project-coordinate-handlers.md`](../../../../docs/dev/todo/2026-10-04-keyring-assignments-grew-project-coordinate-handlers.md)
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 528 | first detection. The size predates #509 (528 at merge-base `4e7157d2`). #509 changed 10 lines (5+/5−), all of them the `os_user_for_github` binding (`let os_user = self…` → `&self…`). It did not grow the file |
| 2026-10-04 | 650 | **grown by #512** (`#keyring` 5/9): +122 for `set_project_accounts_at_project_coordinate`, the sibling of `set_project_default_branch_at_project_coordinate`. No dependent #513–#516 touches the file; the split was deferred by the developer, not blocked |
| 2026-10-24 | 586 | touched by `fix-main-agent-harness-se-e79c8de0` (`c1df8e142..4348f160a`) and **unchanged by it**: two `home_agent: None` → `home_session_id: None` field renames (`:170`, `:328`), 586 production lines at both. `/analyze-code-issues` re-run on `tddy-daemon-rpc` (287 tests across 85 files, join 66.8%): `create_project_at_project_coordinate` (CRAP 240) and `list_project_branches_at_project_coordinate` (CRAP 110) in this file are never entered. They are the two top CRAP findings in the crate, with records of their own |

## What would close it — candidate seams (not proven)

The file is one inherent `impl ProjectRpcHandler` (L26–528), so its methods can move to sibling files without delegating stubs.

| Seam | Items | Out |
|---|---|---|
| A | `add_project_to_host_at_project_coordinate` (L186–360), which already has its own record: `complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate.md` | ~175 |
| B | branch operations: `set_project_default_branch_at_project_coordinate`, `list_project_branches_at_project_coordinate`, and since #512 `set_project_accounts_at_project_coordinate` | ~285 |
| C | `list_projects_at_project_coordinate`, `create_project_at_project_coordinate` (L27–185) | ~160 |

From this first reading, Seam A alone brings the file to about 355 lines. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. No dependent is known to touch this file. #510 does edit its session-lifecycle counterpart, `connection_service/project_coordinate_handlers.rs`.

## Measurement update 2026-10-15 — #622 (local gh)

**Regressed, split deferred with consent:** 650 → 658 production lines. `SetProjectAccounts` carries `local_gh` and maps the storage refusal to `InvalidArgument`. See [2026-10-15-local-gh-oversized-files-deferred.md](../../../../docs/dev/todo/2026-10-15-local-gh-oversized-files-deferred.md).

## Measurement update 2026-10-17 — clone passphrase prompt

**Regressed, split deferred with consent:** 658 → 660 production lines. +2: the two `CloneRequest` constructions gained the `env` field the forked worker now carries (`env: Default::default()`). The change's own
modules are all small and new; only this unavoidable wiring landed in the file. Not claimed by any PR.
Deferred in `docs/dev/todo/2026-10-17-oversized-files-this-change-grew-by-wiring.md`.

## Measurement update 2026-10-18 — `#satellite` 5/13

**Touched, unchanged:** 660 → 660 production lines. #627 repoints one `local_instance_id_for_config` import at `tddy_daemon_kernel::daemon_identity`; no line count moves. Split not attempted (move-only node). Not claimed by any PR.

## Measurement update 2026-10-20 — `#satellite` 10/13 (#632)

660 → 652 production lines (same `awk` count on base and HEAD): the three forwards name `tddy_daemon_kernel::peer_forwarding` / `peer_calls` instead of the LiveKit plugin, and the shorter paths rustfmt onto fewer lines. Import rewrites only; still over budget, split still not designed.

## Measurement update 2026-10-23 — `#project-home` 1/7 (#696)

**Better, not clean:** 652 → 586 production lines (same `awk` count on merge-base and HEAD). #696
adds `home_agent: None` to two `ProjectData` literals (+2) and moves the peer-routing block the three
per-host RPCs each carried into `project/coordinate_route.rs` (`coordinate_route`, −68); its own RPC
lives in the new `project/home_agent_handler.rs`. Still 86 over budget; Seam A
(`add_project_to_host_at_project_coordinate`, now 153 lines) alone would close it. Not claimed by any PR.

## Measurement update 2026-10-24 — `#start-progress` 5/8 (#738)

**Better, not clean:** 586 → 564 production lines (same `awk` count on merge-base and HEAD). The two
inline clone dispatches in `create_project_at_project_coordinate` and
`add_project_to_host_at_project_coordinate`, each wrapped in a deadline, became one
`clone_without_deadline` method (−22). Still 64 over budget; Seam A (`add_project_to_host_at_project_coordinate`,
now 119 lines) alone would close it. Not split here (alert-only: this PR did not grow the file). Not
claimed by any PR.
