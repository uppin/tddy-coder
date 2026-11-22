# oversized-file: test_util.rs

**Location:** `packages/tddy-daemon-rpc/src/test_util.rs`
**Category:** oversized-file
**Detected:** 2026-11-22 — `/pr-wrap` step 3.5 file-length gate on #1007 (`#workqueue` 6/14), production lines counted to the first `#[cfg(test)]` (there is none)
**Metrics:** **552 production lines** of 552 total · budget 500 · **1.1× over**
**Thresholds breached:** length 552 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** line count machine-measured at HEAD and the merge-base with `feature/workqueue/session-work-queue`

The file is the crate's shared test daemon (`pub mod test_util`, compiled into the library so other
crates' suites can use it). Most of its length is one `impl WorkQueueService for TestDaemon` that
forwards every RPC of the family to the real handlers, one nine-line method per RPC — so it grows by
nine lines with every RPC the family gains.

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-22 | 500 → 552 | **Crossed the budget** in #1007 (`#workqueue` 6/14): forwarders for `AssignWorkItemPr`, `UnassignWorkItemPr`, `LinkWorkItemSession`, `UnlinkWorkItemSession` and `StreamWorkQueueStatus`. Not split: the file is stack-shared — #1004 (n3) and #1006 (n5) edited it, and every later `#workqueue` node's branch does — so the stack-branch rule defers the split to the post-stack sweep |
| 2026-11-22 | 552 → 582 | **Grew by 30** in #1008 (`#workqueue` 7/14): forwarders for `SetWorkItemLabels`, `SetWorkItemBadge` and `ClearWorkItemBadge`. Not split: stack-shared with every later `#workqueue` node, so the stack-branch rule defers the split to the post-stack sweep |

## What would close it — candidate seam (not proven)

Move the `WorkQueueService` forwarding impl (with its imports) into `test_util/work_queue.rs`, a child
module of `test_util` — about 150 lines out, leaving ~400. A trait impl moves whole, so this is one
`move_item`; prove it with `restructure check --deep` after the `#workqueue` stack lands.
