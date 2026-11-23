# oversized-file: exec_tool/work_item_git_tools.rs

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/work_item_git_tools.rs`
**Category:** oversized-file
**Detected:** 2026-11-23 — `/pr-wrap` step 3.5 file-length gate on #1015 (`#workqueue` 14/14), production lines counted to the first `#[cfg(test)]` (whole file for TypeScript)
**Metrics:** **529 production lines** · budget 500
**Thresholds breached:** length 529 > 500
**Restructure:** not designed
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-23 | 499 → 529 | **Crossed the budget** in #1015: the `Propagate` and `Land` arms of `run_stack_git_op` (`Land` reads the merge hold-ups first) and the `ActivitySink` the git work publishes landings and refusals through. Not split: the file is stack-shared (`#workqueue` nodes 3–13 edited it), so the stack-branch rule defers the split to the post-stack sweep |

## What would close it — candidate seams (not proven)

Move `DaemonStackGitTools::{spawn_child, delegate_conflicts}` (the session-side legs) into a sibling `work_item_git_tools/session_legs.rs`, leaving the port impl and `run_blocking`; about 120 lines out. Prove with `restructure check --deep` after the stack lands.
