# oversized-file: pr_stack/ports.rs

**Location:** `packages/tddy-daemon-rpc/src/pr_stack/ports.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 — `/pr-wrap` step 3.5 file-length gate on #509 (`#keyring` 2/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **775 production lines** of 775 total (no `#[cfg(test)]`; 0 test lines) · budget 500 · **1.6× over**
**Thresholds breached:** length 775 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 775 | first detection. The size predates #509 (775 at merge-base `4e7157d2`). #509 changed 12 lines (6+/6−), all of them the `os_user_for_github` binding (`let os_user = self…` → `&self…`, for the account-identity return type). It did not grow the file |

## What would close it — candidate seams (not proven)

The file is one `impl PrStackHandler for PrStackRpcHandler` (L30–775) with eight RPC bodies. A trait impl cannot be split across files as-is. Each seam moves bodies into free functions in a sibling module and leaves delegating one-liners in the impl, the same way `branch_legs`, `guards` and `pr_status` already sit beside it.

| Seam | Items | Out |
|---|---|---|
| A | read side: `get_pr_status`, `query_branch`, `resolve_stack_base` (L95–365) | ~260 |
| B | planned-PR mutations: `add_planned_pr`, `link_stack_node`, `repoint_planned_pr`, `reorder_planned_pr` (L31–94, L366–611) | ~300 |
| C | `pull_base_into_branch` (L612–774) | ~155 |

From this first reading, Seam A plus Seam C bring the file to about 380 lines with stubs. Prove the seams with `restructure check --deep` before applying them.

**Stack constraint:** `#keyring` dependents #510–#516 are still open, so no split lands until that stack does. No dependent is known to touch this file.
