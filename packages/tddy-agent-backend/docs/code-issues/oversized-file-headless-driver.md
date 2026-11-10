# oversized-file: headless_driver.rs

**Location:** `packages/tddy-agent-backend/src/backend/headless_driver.rs`
**Category:** oversized-file
**Detected:** 2026-11-02 — `/pr-wrap` step 3.5 file-length gate on #795 (`#agent-tools` 6/12), production lines counted to the first `#[cfg(test)]`
**Metrics:** **509 production lines** (the file has no `#[cfg(test)]`; its tests are `tests/headless_driver_acceptance.rs`) · budget 500 · 1.02× over
**Thresholds breached:** length 509 > 500
**Restructure:** not designed
**Status:** Open — regressed 2026-11-05, deferred to the #agent-tools sweep node
**Verified:** machine-measured (`wc -l`, 2026-11-05)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-02 | 432 → 547 | pushed over budget by #795: `HeadlessCommand.{renderer, scratch_dir}`, `run_rendered_turn` and the `read_turn` split. `argv.rs::claude()` and the hand-built turn stay while `tests/headless_driver_acceptance.rs` drives a renderer-less driver |
| 2026-11-03 | 547 → 563 | #798 (`#agent-tools` 9/12): Cursor, Codex and Oz turns render from their specs; `headless_driver/argv.rs` stays |
| 2026-11-05 | 563 → 477 | #821 (`#agent-tools` 15/19), green: `argv.rs`, the renderer-less turn, `HeadlessHarness` and the driver's own spawn code deleted (the spawn moved to `backend/turn_process.rs`). Under budget |
| 2026-11-05 | 477 → 509 | **regressed** at `/validate-changes` on #821: a conversation turn's deadline (`HeadlessCommand::turn_deadline`, the overrun arms of both `select!` loops, `end_overrun`); `STDERR_GRACE` moved out to `turn_process` |
| 2026-11-07 | 509 → 505 | better by 4 in #829 (`#agent-tools` 21/28): the `TODO(headless-driver)` on `HeadlessCommand::env` went with the tool-server env it described, `headless_server` passes `cwd`. Still over; not grown, so not split here |

## What would close it

Move the stderr reader (`drain_stderr`, the stderr tail) and `end_overrun` / `kill_turn` into
`backend/turn_process.rs` beside the workflow backend's, or `read_turn` into a `headless_driver/`
sibling (engine-driven, `code-restructuring`). Its two long functions have their own record,
`complexity-headless-driver-run-turn.md`.
