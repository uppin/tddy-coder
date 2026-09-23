# oversized-file: workflow_runner.rs

**Location:** `packages/tddy-presenter/src/presenter/workflow_runner.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **1,015 production lines** of 1,015 total (no `#[cfg(test)]`) · budget 500 · **2.0× over**
**Thresholds breached:** length 1015 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/presenter/workflow_runner.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 1015 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: one line changed: the Cut 2 call path `crate::changeset::start_goal_for_session_continue` → `crate::workflow::…` |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | post-workflow elicitation: `ElicitationContext`, `handle_clarification_round`, `run_post_workflow_elicitation_rounds`, `handle_elicitation` (L22–355) | ~335 |
| B | `run_start_goal_without_output_dir` (L356–633) | ~280 |
| C | `run_workflow` (L634–1015) | ~380 |

Three functions carry the file, and each has its own complexity record (`complexity-workflow-runner-handle-elicitation.md`, `…-run-start-goal-without-output-dir.md`, `…-run-workflow.md`). A three-module split gets every piece under budget. `packages/tddy-integration-tests/tests/workflow_goal_conditions_acceptance.rs` reads this file by path with `include_str!`, so a split must retarget that test too. Prove the seams with `restructure check --deep` before applying them.
