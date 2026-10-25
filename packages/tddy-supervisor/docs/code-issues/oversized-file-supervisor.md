# oversized-file: supervisor.rs

**Location:** `packages/tddy-supervisor/src/supervisor.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 — `/pr-wrap` step 3.5 file-length gate on #736 (`#start-progress` 3/8), production lines counted to the first `#[cfg(test)]`
**Metrics:** **1019 production lines** · budget 500 · already over at merge-base (965)
**Thresholds breached:** length 1019 > 500
**Restructure:** not designed
**Status:** Closed 2026-10-25 by #721's split (measured after the rebase onto #start-progress) — under budget; delete at #721's wrap, recording this final measurement in its change-history entry
**Verified:** ⚠ **not hand-verified** — machine-measured line count

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-24 | 965 → 972 | grown by #736: Slot::spawn_plan supplying supervisor_socket to EnvironmentBase::Inherited |
| 2026-10-24 | 972 → 1019 | grown by #739: `spawn_session_capturing_stderr`, `wait_for_session_exit` — split deferred with the developer's consent, #start-progress 6/8 |
| 2026-10-25 | 1019 → 379 | closed by #721 (`#coagents` 2/8), rebased onto #start-progress: its split moved the session table and the session spawn methods to `supervisor/session_table.rs` (419, now with `spawn_session_capturing_stderr` and `wait_for_session_exit`), `Slot` to `service_slot.rs` (142) and the restart path to `service_restart.rs` (128) |
