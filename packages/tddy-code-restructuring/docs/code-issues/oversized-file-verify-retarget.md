# oversized-file: verify/retarget.rs — retarget accounting plus the delegator rule

**Location:** `packages/tddy-code-restructuring/src/verify/retarget.rs`
**Category:** oversized-file
**Detected:** 2026-10-14 by the `/pr-wrap` file-length gate on #603 (`#reshape` 6/19)
**Metrics:** **555 production lines** (2026-10-14; 341 before #603) · budget 500
**Restructure:** required
**Status:** Open
**Claimed by:** none

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-14 | 555 | #603 (`#reshape` 6/19): R3 (delegator signatures and forwarding calls) added about 214 lines, counted to the first `#[cfg(test)]`. 341 at the merge base |

## What the gate found

#603 pushed the file from 341 to 555 production lines, 55 over the budget.

## Why it was not split in #603

The `/pr-wrap` stack-overlap stop: `feature/reshape/backend-session` (#reshape 18/19) and
`feature/reshape/fn-sizes-backend` also edit this file. Deferred with the developer's consent
(2026-10-14), recorded in `docs/dev/todo/2026-10-14-restructure-verify-retarget-rs-exceeds-the-500-line-budget.md`.

## What would close it

After the stack lands, move the R3 pass into a sibling module under `verify/retarget/` using engine
moves, bringing `retarget.rs` under 500 production lines.
