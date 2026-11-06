# complexity: resume_sandboxed_claude_cli_session

**Location:** `packages/tddy-agent-launch/src/svc_resume_sandboxed_claude_cli_session.rs:11` — `resume_sandboxed_claude_cli_session`
**Category:** complexity
**Detected:** 2026-11-05 — `/pr-wrap` on #819 (`#agent-tools` 13/19), fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run
**Metrics:** **79 lines**
**Thresholds breached:** length 79 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 79 → 79 | first record — over the threshold before #819. #819 touched it at the route call (`RouteAgent::claude_on` in place of `Harness::ClaudeCli`) and did not grow it |

## What would close it

Extract the jail relaunch (stop, re-resolve the route, respawn) into a helper beside `resume_sandboxed_cli_harness`, which does the same steps for the generic CLI harness.
