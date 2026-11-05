# oversized-file: acp_driver.rs

**Location:** `packages/tddy-agent-backend/src/backend/acp_driver.rs`
**Category:** oversized-file
**Detected:** 2026-11-05 — `/pr-wrap` step 3.5 file-length gate on #822 (`#agent-tools` 16/19), production lines counted to the first `#[cfg(test)]`
**Metrics:** **504 production lines** · budget 500 · 1.01× over
**Thresholds breached:** length 504 > 500
**Restructure:** not designed
**Status:** Open — split deferred to the `#agent-tools` sweep node (n20) by orchestrator instruction; **unclaimed**
**Verified:** machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-05 | 459 → 504 | #822: `SESSION_OPEN_DEADLINE`, `humantime_deadline`, `open_session_within`, `open_session_until` — the session-open deadline. Pushed the file past budget; restructuring in this stack is the sweep's |

## What would close it

Move the driver's public value types (`AcpAgentCommand`, `OpenSession`, `AcpSession`,
`PermissionRequest`/`PermissionDecision` and friends) into a sibling module re-exported from here,
or the deadline helpers next to `session.rs`, taking the file under 500.
