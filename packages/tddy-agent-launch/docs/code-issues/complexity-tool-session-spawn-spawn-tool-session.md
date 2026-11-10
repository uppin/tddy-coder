# complexity: spawn_tool_session

**Location:** `packages/tddy-agent-launch/src/tool_session_spawn.rs:31` — `spawn_tool_session`
**Category:** complexity
**Detected:** 2026-11-05 — `/pr-wrap` on #819 (`#agent-tools` 13/19), fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run
**Metrics:** **142 lines**
**Thresholds breached:** length 142 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 141 → 142 | first record — over the threshold before #819. **Worse by 1** in #819: `tool_session_route_json` takes the host's coding agents (one argument line) |

## What would close it

Extract the route resolution and the tool-session argv into helpers; the function assembles the spawn from them.
