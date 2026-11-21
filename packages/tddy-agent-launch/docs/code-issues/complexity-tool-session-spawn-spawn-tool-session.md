# complexity: spawn_tool_session

**Location:** `packages/tddy-agent-launch/src/tool_session_spawn.rs:31` — `spawn_tool_session`
**Category:** complexity
**Detected:** 2026-11-05 — `/pr-wrap` on #819 (`#agent-tools` 13/19), fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run
**Metrics:** **141 lines** (142 on 2026-11-05)
**Thresholds breached:** length 142 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 141 → 142 | first record — over the threshold before #819. **Worse by 1** in #819: `tool_session_route_json` takes the host's coding agents (one argument line) |
| 2026-11-20 | 142 → 141 | #960 (`#session-liveness` 3/4): **better by 1** — `tool_session_spawned` (the process-tab registration) took the spawn result by value in place of the two-line `let result = spawned?`; `spawn_tddy_coder` was split into one function per backend (85 → 10 lines). Fn line to closing brace on `origin/feature/session-liveness/tmux-roster` (`dfe3ee833`) and HEAD; no coverage run. Not split further: this record's restructure is still the route-resolution and argv extraction |

## What would close it

Extract the route resolution and the tool-session argv into helpers; the function assembles the spawn from them.
