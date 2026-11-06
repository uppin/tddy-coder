# complexity: resume_cli_harness_session

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn.rs:377` — `resume_cli_harness_session`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **74 lines**
**Thresholds breached:** length 74 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 61 → 74 | #797: the host resume renders the spec's resume argv and wires the resume adapter's `on_exit` hook |

## What would close it

Share the spec render and hook wiring with `start_cli_harness_session` (one extracted helper), which takes both under 60.
