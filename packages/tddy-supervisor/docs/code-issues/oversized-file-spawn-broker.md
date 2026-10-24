# oversized-file: spawn_broker.rs

**Location:** `packages/tddy-supervisor/src/spawn_broker.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 — `/pr-wrap` step 3.5 file-length gate on #736 (`#start-progress` 3/8), production lines counted to the first `#[cfg(test)]`
**Metrics:** **1549 production lines** · budget 500 · already over at merge-base (1526)
**Thresholds breached:** length 1549 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed** — split deferred with the developer's consent, #start-progress 3/8. The file is touched by other `#start-progress` nodes, so a split here would cascade rename conflicts through the stack; do it after the stack lands.
**Verified:** ⚠ **not hand-verified** — machine-measured line count

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-24 | 1526 → 1549 | grown by #736: ChildEnvironment / EnvironmentBase::Inherited carrying the supervisor socket, and the strip-then-insert of TDDY_SUPERVISOR_SOCKET |
