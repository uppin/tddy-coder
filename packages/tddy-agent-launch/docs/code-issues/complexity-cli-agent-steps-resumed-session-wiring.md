# complexity: resumed_session_wiring

**Location:** `packages/tddy-agent-launch/src/cli_agent_steps.rs:444` — `LaunchSessions::resumed_session_wiring`
**Category:** complexity
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **72 lines** (fn line to closing brace)
**Thresholds breached:** length 72 > 60 (`/analyze-clean-code`)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 0 → 72 | first detection — #820: the resumed managed workflow (recipe, resume goal, `prepare_managed_workflow`), the resumed notify wiring and the resumed role. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |

## What would close it

Extract the resumed managed-workflow block into a helper with `code-restructuring` (`extract_method`), and share the role/tool-server tail with `wire_session_agent`.
