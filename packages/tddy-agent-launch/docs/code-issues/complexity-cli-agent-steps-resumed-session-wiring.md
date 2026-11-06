# complexity: resumed_session_wiring

**Location:** `packages/tddy-agent-launch/src/cli_agent_steps.rs:465` — `LaunchSessions::resumed_session_wiring`
**Category:** complexity
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **79 lines** (fn line to closing brace)
**Thresholds breached:** length 72 > 60 (`/analyze-clean-code`)
**Status:** Open — regressed 2026-11-06 by #823 (72 → 79) — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 0 → 72 | first detection — #820: the resumed managed workflow (recipe, resume goal, `prepare_managed_workflow`), the resumed notify wiring and the resumed role. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 72 → 79 | **worse by 7** in #823 (`#agent-tools` 17/19): as `wire_session_agent`, the resumed notify pairs go to the session pointer; measured fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |

## What would close it

Extract the resumed managed-workflow block into a helper with `code-restructuring` (`extract_method`), and share the role/tool-server tail with `wire_session_agent`.
