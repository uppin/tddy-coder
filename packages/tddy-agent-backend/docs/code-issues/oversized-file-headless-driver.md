# oversized-file: headless_driver.rs

**Location:** `packages/tddy-agent-backend/src/backend/headless_driver.rs`
**Category:** oversized-file
**Detected:** 2026-11-02 — `/pr-wrap` step 3.5 file-length gate on #795 (`#agent-tools` 6/12), production lines counted to the first `#[cfg(test)]`
**Metrics:** **547 production lines** · budget 500 · 1.1× over
**Thresholds breached:** length 547 > 500
**Restructure:** not designed
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Verified:** machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-02 | 432 → 547 | pushed over budget by #795: `HeadlessCommand.{renderer, scratch_dir}`, `run_rendered_turn` and the `read_turn` split. `argv.rs::claude()` and the hand-built turn stay while `tests/headless_driver_acceptance.rs` drives a renderer-less driver |

## What would close it

Move `run_rendered_turn` and its `HeadlessTurn` assembly into a `headless_driver/rendered.rs` sibling (engine-driven, `code-restructuring`), or delete the hand-built Claude turn once its tests use a renderer (#798 moves Cursor, Codex and Oz onto the spec too).

## Measurement update 2026-11-03 — #agent-tools 9/12 (#798)

**Regressed:** 547 → 563 production lines: Cursor, Codex and Oz turns render from their specs. `headless_driver/argv.rs` stays until its deletion is approved (`docs/dev/todo/2026-11-03-headless-others-legacy-argv-deletion.md`). Regressed by #798 (`#agent-tools` 9/12). **Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-03).
