# oversized-file: runner/entry_points/store_run.rs — the apply loop over a plan store

**Location:** `packages/tddy-code-restructuring/src/runner/entry_points/store_run.rs`
**Category:** oversized-file
**Detected:** 2026-10-22 by the `/pr-wrap` file-length gate on #681 (`#reshape` 22/24)
**Metrics:** **501 production lines** (2026-10-22, counted to the first `#[cfg(test)]`; 507 at the merge base) · budget 500
**Restructure:** required
**Status:** Open — pre-existing; #681 touched it and shrank it
**Claimed by:** none

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-22 | 507 → 501 | #681 (`#reshape` 22/24): the apply's registry is built by `registry_for_apply` (one call) instead of `registry_for` with four arguments. Already over the budget at the base, which #681 did not grow |

## What would close it

Two lines over. Not analysed for seams yet: the next planner's Step 2b question.
