# oversized-file: project/coordinate_handlers.rs

**Location:** `packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **528 production lines** of 528 total (no `#[cfg(test)]`; 0 test lines) · budget 500 · **1.1× over**
**Thresholds breached:** length 528 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 528 | first detection. The size predates #509 (528 at merge-base `4e7157d2`). #509 changed 10 lines (5+/5−), all of them the `os_user_for_github` binding (`let os_user = self…` → `&self…`). It did not grow the file |

## What would close it — candidate seams (not proven)

The file is one inherent `impl ProjectRpcHandler` (L26–528), so its methods can move to sibling files without delegating stubs.

| Seam | Items | Out |
|---|---|---|
| A | `add_project_to_host_at_project_coordinate` (L186–360), which already has its own record: `complexity-project-coordinate-handlers-add-project-to-host-at-project-coordinate.md` | ~175 |
| B | branch operations: `set_project_default_branch_at_project_coordinate`, `list_project_branches_at_project_coordinate` (L361–527) | ~165 |
| C | `list_projects_at_project_coordinate`, `create_project_at_project_coordinate` (L27–185) | ~160 |

From this first reading, Seam A alone brings the file to about 355 lines. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. No dependent is known to touch this file. #510 does edit its session-lifecycle counterpart, `connection_service/project_coordinate_handlers.rs`.
