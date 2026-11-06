# complexity: wire_session_agent

**Location:** `packages/tddy-agent-launch/src/cli_agent_steps.rs:347` — `wire_session_agent`
**Category:** complexity
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **80 lines** (fn line to closing brace)
**Thresholds breached:** length 80 > 60 (`/analyze-clean-code`)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 0 → 80 | first detection — #820: the hooks, the managed workflow (`prepare_managed_workflow_inner`, eleven arguments), the operator-notify guidance and the role, each routing its `TDDY_*` pairs to the tool server. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |

## What would close it

Extract the managed-workflow block into `wire_managed_workflow(&ManagedWiring, …) -> ManagedLaunch` and the notify block into a helper with `code-restructuring` (`extract_method`); the shared tail (role toolset, tool-server env) is the same in `resumed_session_wiring` and can be one helper for both.
