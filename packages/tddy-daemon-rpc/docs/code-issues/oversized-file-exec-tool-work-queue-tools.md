# oversized-file: exec_tool/work_queue_tools.rs

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/work_queue_tools.rs`
**Category:** oversized-file
**Detected:** 2026-11-23 — `/pr-wrap` step 3.5 file-length gate on #1015 (`#workqueue` 14/14), production lines counted to the first `#[cfg(test)]` (whole file for TypeScript)
**Metrics:** **647 production lines** · budget 500
**Thresholds breached:** length 647 > 500
**Restructure:** not designed
**Status:** Closed — `#workqueue` 15/15 sweep (#1022): 312 lines — inherent `impl DaemonStackTools` moved to `work_queue_tools/queue_legs.rs`; `ops.rs` 510 → 349 (item edits moved to `ops/item_edits.rs`, `ops/annotation_edits.rs`). Record deleted at wrap.
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-23 | 547 → 647 | **Regressed by 100** in #1015: arming or disarming through the tools starts the queue's schedule watcher (`schedule_watch::after_arming`, with the caller as an `Armer`), and `merge_holdups` for `work_queue_land`. Not split: the file is stack-shared (`#workqueue` nodes 3–13 edited it), so the stack-branch rule defers the split to the post-stack sweep |

## What would close it — candidate seams (not proven)

The schedule leg (`run_schedule_op` and its watcher start) and the status-through-inbox leg (`status_through_inbox`, `refresh_through_inbox`) are each a cohesive `impl DaemonStackTools` block; move each into a sibling module under `work_queue_tools/` — about 220 lines out. Prove with `restructure check --deep` after the stack lands.
