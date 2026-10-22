# oversized-file: verify/retarget.rs — retarget accounting plus the delegator rule

**Location:** `packages/tddy-code-restructuring/src/verify/retarget.rs`
**Category:** oversized-file
**Detected:** 2026-10-14 by the `/pr-wrap` file-length gate on #603 (`#reshape` 6/19)
**Metrics:** **555 production lines** (2026-10-14; 341 before #603) · budget 500
**Restructure:** required
**Status:** Open — regressed 2026-10-21 (#677, `#reshape` 15/24: `Declared.detaches` and `Declared::with_detaches`)
**Claimed by:** none

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-14 | 555 | #603 (`#reshape` 6/19): R3 (delegator signatures and forwarding calls) added about 214 lines, counted to the first `#[cfg(test)]`. 341 at the merge base |
| 2026-10-21 | 631 | #677 (`#reshape` 15/24, `detach-method`): +30 over the 601 at its base `feature/reshape/move-widen` (counted to the first `#[cfg(test)]`). The `detaches` field and the `with_detaches` builder, which adds the rebind a detach implies; R-detach itself is `verify/detach.rs`. Split not done here: stack-shared, and `#reshape` 17 owns the split of this file |
| 2026-10-22 | 631 | unchanged by #612 (`#reshape` 17/24), which did not touch the file; measured by `tddy-tools restructure lines`. The split (Plan D) is deferred by the developer to the `#reshape` sweep node; seams in `docs/dev/todo/2026-10-21-reshape-oversized-files-splits-deferred.md` |

## What the gate found

#603 pushed the file from 341 to 555 production lines, 55 over the budget.

## Why it was not split in #603

The `/pr-wrap` stack-overlap stop: `feature/reshape/backend-session` (#reshape 18/19) and
`feature/reshape/fn-sizes-backend` also edit this file. Deferred with the developer's consent
(2026-10-14), recorded in `docs/dev/todo/2026-10-14-restructure-verify-retarget-rs-exceeds-the-500-line-budget.md`.

## What would close it

After the stack lands, move the R3 pass into a sibling module under `verify/retarget/` using engine
moves, bringing `retarget.rs` under 500 production lines.
