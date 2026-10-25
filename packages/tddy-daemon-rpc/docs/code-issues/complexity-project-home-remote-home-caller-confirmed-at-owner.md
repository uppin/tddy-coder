# complexity: home_caller_confirmed_at_owner

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/project_home_remote.rs:82` — `ExecToolRpcHandler::home_caller_confirmed_at_owner`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` on #750 (`#start-parity` 1/9), structural scan (fn line to closing brace)
**Metrics:** **65 lines** · budget 60
**Thresholds breached:** length 65 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-25 | 65 | first detection, added by `#start-parity` 1/9 (#750). Fix deferred by developer instruction for this stack |

## What would close it

Extract the spawn-per-peer `ListSessions` fan-out (`ask_every_peer_for_sessions`) from the owner-row
check; the remaining loop is about 30 lines.
