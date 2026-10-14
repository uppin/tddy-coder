# 2026-10-14 — `verify/retarget.rs` is over the 500-production-line budget after R3

**Category:** Tech debt (file budget; no behaviour is wrong)
**Source:** #reshape 6/19 (`feature/reshape/methods-leave-type`), `/pr-wrap` file-length gate.
Record: `packages/tddy-code-restructuring/docs/code-issues/oversized-file-verify-retarget.md`.

## What happened

R3 (a retarget's delegator signatures and forwarding calls) took
`packages/tddy-code-restructuring/src/verify/retarget.rs` from 341 to 555 production lines. The new
rule belongs with the retarget accounting, so it went in beside it.

## Why it was not split in #reshape 6/19

The stack-overlap stop. `feature/reshape/backend-session` (#reshape 18/19) and
`feature/reshape/fn-sizes-backend` also edit this file; splitting it here would turn their diffs into
conflicts. The developer consented to deferral on 2026-10-14.

## What would close it

After the `#reshape` stack lands, move the R3 pass (delegator signatures, forwarding calls) into a
sibling module under `verify/retarget/` with `restructure` engine moves only, so `retarget.rs` is back
under 500 production lines.
