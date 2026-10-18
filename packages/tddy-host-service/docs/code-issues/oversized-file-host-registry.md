# oversized-file: host_registry.rs

**Location:** `packages/tddy-host-service/src/host_registry.rs`
**Category:** oversized-file
**Detected:** 2026-10-18 — `/pr-wrap` step 3.5 file-length gate on #627 (`#satellite` 5/13), production lines counted to the first `#[cfg(test)]`
**Metrics:** **596 production lines** · budget 500 · **1.19× over**
**Thresholds breached:** length 596 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — machine-measured count only

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-18 | 596 → 596 | #627 changes only a doc comment's identity path; no line count moves. Not grown by this PR, so alert-only |

## What would close it

Not designed; the file has not been read for a seam.
