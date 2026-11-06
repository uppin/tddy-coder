# oversized-file: spec_headless.rs

**Location:** `packages/tddy-agent-backend/src/backend/spec_headless.rs`
**Category:** oversized-file
**Detected:** 2026-11-05 — `/green` on #821 (`#agent-tools` 15/19), production lines counted to the first `#[cfg(test)]` (there is none: its tests are `tests/spec_headless_turn_deadline.rs` and `tests/headless_protocol_unit.rs`)
**Metrics:** **646 production lines** · budget 500 · 1.3× over
**Thresholds breached:** length 645 > 500
**Restructure:** not designed
**Status:** Open — deferred to the #agent-tools sweep node (restructuring is the sweep's, per the stack rules)
**Verified:** machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-05 | 156 → 621 | first detection: the generic headless workflow backend replaced the three bespoke ones (2,524 lines, now deleted). The file holds the backend, its turn assembly from the protocol, the blocking stdout/exit reader and the stderr reader |
| 2026-11-05 | 621 → 645 | `/pr-wrap` on #821: a turn with no working directory is refused, the conversation log opens before the spawn (`open_conversation_log`), a missing stdout reaps the child. Not split: the sweep node's |
| 2026-11-06 | 645 → 646 | **worse by 1** in #823 (`#agent-tools` 17/19): the `notify` field of the pointer `invoke` writes. Restructuring is the sweep's (n20) |

## What would close it

Move `read_turn`, `StderrReader` and `read_stderr` (the blocking reader, ~230 lines) into `backend/spec_headless/read.rs`, and `turn_for`/`system_text`/`child_env` (the request → `HeadlessTurn` mapping, ~150 lines) into `backend/spec_headless/turn.rs` (engine-driven, `code-restructuring`).
