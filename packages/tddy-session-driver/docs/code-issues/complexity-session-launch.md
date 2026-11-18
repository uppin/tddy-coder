# complexity: DriverSession::launch

**Location:** `packages/tddy-session-driver/src/session.rs:99` — `DriverSession::launch`
**Category:** complexity
**Detected:** 2026-11-18 — `/yolo-pr-stack` sweep-collect (`#coder-start`): length by brace matching; `tddy-tools analyze report` for complexity
**Metrics:** **61 lines** · complexity 7 · budget 60 lines — **1 over**. Report CRAP 56.0 is **not trusted**: see "Verified by hand"
**Thresholds breached:** length 61 > 60
**Restructure:** `extract_method` — the local-target arm (launch, resize, startup-window wait, exit check, ~25 lines) into its own `launch_local`
**Status:** Open — new in the `#coder-start` stack (new package); marginal — **unclaimed**

## Measurement history

| Run | Lines | Complexity | Note |
|---|---|---|---|
| 2026-11-18 | 61 | 7 | first detection; new package |

## What the tool found

The remote arm calls `launch_on_target`; the local arm calls `launch_host_pty` then judges the startup window itself with a poll loop. Join rate for the package 67.6% (50 of 74).

## Why it matters here

One line over, so low priority. The local arm duplicates, by hand, what the remote launch does inside `wait_for_agent_start`.

## What would close it

Extract the local arm; the function drops to about 35 lines.

## Verified by hand

2026-11-18: opened the function. The reported CRAP 56.0 (complexity 7, 0% coverage) is a **tool blind spot**: `tests/session_driver_acceptance.rs` (7 tests, all passing) drives `launch`, `stop` and `wait_for` by spawning the `tddy-session-driver` binary (`CARGO_BIN_EXE_tddy-session-driver`), whose process the in-process coverage capture does not instrument. `stop` (CRAP 56.0) and `wait_for` (42.0) read the same way. They are not recorded as `crap` findings.
