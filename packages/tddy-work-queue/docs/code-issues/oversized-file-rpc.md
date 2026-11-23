# oversized-file: rpc.rs

**Location:** `packages/tddy-work-queue/src/rpc.rs`
**Category:** oversized-file
**Detected:** 2026-11-23 — `/pr-wrap` step 3.5 file-length gate on #1015 (`#workqueue` 14/14), production lines counted to the first `#[cfg(test)]` (whole file for TypeScript)
**Metrics:** **514 production lines** · budget 500
**Thresholds breached:** length 514 > 500
**Restructure:** not designed
**Status:** Closed — `#workqueue` 15/15 sweep (#1022): 273 lines — `WorkQueueServiceImpl` and `build_pr_stack_entry` moved to `rpc/service.rs`, re-exported. Record deleted at wrap.
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-23 | 483 → 514 | **Crossed the budget** in #1015: the `propagate_work_item` / `land_work_queue` trait methods and their forwarding impls. The file grows by two lines per RPC on the trait and ~9 per forwarder with every RPC the family gains. Not split: the file is stack-shared (`#workqueue` nodes 3–13 edited it), so the stack-branch rule defers the split to the post-stack sweep |

## What would close it — candidate seams (not proven)

Split the `WorkQueueHandler` trait from `WorkQueueServiceImpl`'s forwarding `impl WorkQueueService` into `rpc/handler.rs` and `rpc/service.rs` — a whole-item move each.
