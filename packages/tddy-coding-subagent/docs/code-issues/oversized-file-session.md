# oversized-file: session.rs

**Location:** `packages/tddy-coding-subagent/src/session.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 by `/pr-wrap` step 3.5 file-length gate on PR #722 (`#coagents` 3/8)
**Metrics:** **501 production lines** · budget 500 · **1.00× over**
**Thresholds breached:** length 501 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-24 | 323 → 501 | first detection — PR #722 crossed it: the in-flight prompt held across tddy turns (`InFlightPrompt`, `settle`, `resume`, `finish`) and the cancel that drives a paused prompt to its end. Not split: stack-shared — later `#coagents` nodes modify this file, so a decomposition here would conflict through their diffs |

## What would close it

The cancel path — `CancelHandle`, `CANCEL_SETTLE_DEADLINE` and `drive_cancelled_turn_to_its_end` —
is one cohesive group (~70 lines) that only shares `InFlightPrompt` with the rest; moving it to a
`cancel.rs` sibling (engine-driven, `code-restructuring`) after the `#coagents` stack lands takes the
file under budget.
