# oversized-file: mount_host.rs

**Location:** `packages/tddy-daemon/src/runtime/mount_host.rs`
**Category:** oversized-file
**Detected:** 2026-11-17 — `/pr-wrap` step 3.5 file-length gate on #930 (`#activity-wake` 2/3), production lines counted to the first `#[cfg(test)]`
**Metrics:** **516 → 527 production lines** (2026-11-17) · budget 500
**Thresholds breached:** length 514 > 500
**Restructure:** `move_item` into a submodule — `/code-restructuring` territory
**Status:** Open — deferred to a follow-up branch after the `#activity-wake` stack lands; unclaimed

Not split in #930: predecessor PR #929 (`feature/activity-wake/session-ended-notice`) and successor
PR #931 (`feature/activity-wake/idle-wake`) also modify this file, so the split is deferred under
`/pr-wrap` step 3.5's stack-branch rule. #930 grew it by opening the session-watch store beside
the notification log (`open_session_watches`, and the `Shared` field it fills).

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-11-17 | 516 → 527 | #930: the session-watch store is opened in `open_shared` |
| 2026-11-17 | 527 → 532 | #931: the wake trigger is registered on the bus in `open_shared`. Still stack-shared — successors #934/#935 also modify this file — so the split stays deferred to a follow-up after the stack lands |

## What would close it

Move `open_session_watches` and the other `open_*` helpers of `open_shared` into a
`runtime/mount_host/open_shared.rs` submodule with `tddy-tools restructure` intents proved by
`restructure check --deep`, and confirm the file and the new module are each ≤ 500 production
lines.
