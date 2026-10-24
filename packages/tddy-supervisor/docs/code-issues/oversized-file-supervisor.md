# oversized-file: supervisor.rs

**Location:** `packages/tddy-supervisor/src/supervisor.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 — `/pr-wrap` step 3.5 file-length gate on #736 (`#start-progress` 3/8), production lines counted to the first `#[cfg(test)]`
**Metrics:** **1019 production lines** · budget 500 · already over at merge-base (965)
**Thresholds breached:** length 1019 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed** — split deferred with the developer's consent, #start-progress 3/8 and 6/8. The file is touched by other `#start-progress` nodes, so a split here would cascade rename conflicts through the stack; do it after the stack lands.
**Verified:** ⚠ **not hand-verified** — machine-measured line count

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-24 | 965 → 972 | grown by #736: Slot::spawn_plan supplying supervisor_socket to EnvironmentBase::Inherited |
| 2026-10-24 | 972 → 1019 | grown by #739: `spawn_session_capturing_stderr`, `wait_for_session_exit` — split deferred with the developer's consent, #start-progress 6/8 |
