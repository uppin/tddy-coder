# complexity: resume_sandboxed_cli_harness

**Location:** `packages/tddy-agent-launch/src/svc_resume_sandboxed_cli_harness_session.rs:42` — `resume_sandboxed_cli_harness`
**Category:** complexity
**Detected:** 2026-11-05 — `/pr-wrap` on #819 (`#agent-tools` 13/19), fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run
**Metrics:** **73 lines**
**Thresholds breached:** length 73 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 72 → 73 | first record — over the threshold before #819. **Worse by 1** in #819: `CliSessionType` is no longer `Copy` (it holds the spec's `RoutedAgent`), so `kind.clone()` wraps the `resolve_session_agent` call onto two lines |

## What would close it

Share the jail relaunch steps with `resume_sandboxed_claude_cli_session` in one helper.
