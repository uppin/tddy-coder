# complexity: wire_session_agent

**Location:** `packages/tddy-agent-launch/src/cli_agent_steps.rs:359` — `wire_session_agent`
**Category:** complexity
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **84 lines** (fn line to closing brace)
**Thresholds breached:** length 80 > 60 (`/analyze-clean-code`)
**Status:** Open — partially fixed 2026-11-07 by #834 (87 → 84), still over 60 — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 0 → 80 | first detection — #820: the hooks, the managed workflow (`prepare_managed_workflow_inner`, eleven arguments), the operator-notify guidance and the role, each routing its `TDDY_*` pairs to the tool server. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 80 → 87 | **worse by 7** in #823 (`#agent-tools` 17/19): the operator-notify pairs go to the session pointer, not the agent process (`notify_env`, `session_pointer_of`); measured fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |
| 2026-11-07 | 87 → 84 | **better by 3** in #834 (`#agent-tools` 26/28): the hooks are given the session user's socket (`agent_tool_socket` input) instead of probing for one, so the `hook_socket` fallback is gone; measured fn line to closing brace on `origin/feature/agent-tools/split-remote-env` (`cd9026d9d`) and HEAD; no coverage run. Still over 60 |

## What would close it

Extract the managed-workflow block into `wire_managed_workflow(&ManagedWiring, …) -> ManagedLaunch` and the notify block into a helper with `code-restructuring` (`extract_method`); the shared tail (role toolset, tool-server env) is the same in `resumed_session_wiring` and can be one helper for both.
