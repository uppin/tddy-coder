# complexity: execute_tool

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/ports.rs:34` — `ExecToolRpcHandler::execute_tool`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` on #750 (`#start-parity` 1/9), structural scan (fn line to closing brace)
**Metrics:** **69 lines** · budget 60
**Thresholds breached:** length 69 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-25 | 69 | first detection: 57 → 69 by `#start-parity` 1/9 (#750), which added the early return for a project-home caller held on another host. Fix deferred by developer instruction for this stack |

## What would close it

The four early-return dispatch arms (peer forward, hosted clone, caller held elsewhere, local run) share
their shape with `stream_execute_tool`'s; one `dispatch_local(&req_conn) -> Result<ExecuteToolResponse, Status>`
called by both handlers takes both under budget and removes the duplication.
