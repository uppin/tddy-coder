# oversized-file: test_util.rs

**Location:** `packages/tddy-session-lifecycle/src/test_util.rs`
**Category:** oversized-file
**Detected:** 2026-11-20 — `/pr-wrap` step 3.5 file-length gate on #958 (`#session-liveness` 1/4), production lines counted to the first `#[cfg(test)]` (the file has none, so all of it counts)
**Metrics:** **556 production lines** · budget 500 · **+56**
**Thresholds breached:** length 556 > 500
**Restructure:** `extract_module --to_file` — `TestDaemon`'s holding helpers (`HeldMain`, `holds_a_main_agent`, `serves_a_main_for`, `stops_holding`, ~95 lines) and the host builders (`test_host`, `test_service`, the coding-agent fixtures) are separate jobs; seam not proven with `check --deep`
**Status:** Open — partially fixed (578 → 556 in #990); crossed 500 in #958 (484 → 578); split deferred under the stack-branch rule — `#session-liveness` 2/4, 3/4 and 4/4 also edit this file — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-20 | 484 → 578 | **Crossed 500** in #958 (`#session-liveness` 1/4): `HeldMain` and `TestDaemon::{holds_a_main_agent, serves_a_main_for, stops_holding}` — the acceptance suites' way to make a session live now that a recorded pid no longer does. Not split: the file is stack-shared with n2–n4 |
| 2026-11-21 | 578 → 556 | **Better, not clean** in #990 (`#tab-hardening` 2/4): the tool-host fixtures (`AHostOfStubs`, `AHostOfBinaries`) became `tddy_test_hosts::a_tool_host()`. Still over; not split — `#tab-hardening` 3/4 and 4/4 also edit this file |

## What would close it

After the stack lands, move `HeldMain` and the `TestDaemon` holding helpers into `test_util/held_agents.rs` (a `mod` declared here and re-exported), leaving the host and config builders in `test_util.rs`. Move-only; every suite imports through `tddy_session_lifecycle::test_util`, so the `pub use` keeps them unchanged.
