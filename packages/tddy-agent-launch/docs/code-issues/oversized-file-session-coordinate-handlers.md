# oversized-file: session_coordinate_handlers.rs

**Location:** `packages/tddy-agent-launch/src/session_coordinate_handlers.rs`
**Category:** oversized-file
**Detected:** 2026-10-18 — `/pr-wrap` step 3.5 file-length gate on #627 (`#satellite` 5/13), production lines counted to the first `#[cfg(test)]`
**Metrics:** **503 production lines** · budget 500 · **1.01× over**
**Thresholds breached:** length 503 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — machine-measured count only

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-18 | 503 → 503 | #627 repoints the `os_user_resolution` import at `tddy_daemon_kernel`; no line count moves. Not grown by this PR, so alert-only |

## What would close it

Three lines. Any one extraction of a handler family into its own module takes it under budget; the file has not been read for a seam.
