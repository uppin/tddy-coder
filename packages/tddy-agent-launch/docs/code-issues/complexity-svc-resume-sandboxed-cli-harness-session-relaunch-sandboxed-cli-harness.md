# complexity: relaunch_sandboxed_cli_harness

**Location:** `packages/tddy-agent-launch/src/svc_resume_sandboxed_cli_harness_session.rs:143` — `relaunch_sandboxed_cli_harness`
**Category:** complexity
**Detected:** 2026-11-06 — `/pr-wrap` on #823 (`#agent-tools` 17/19), fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run
**Metrics:** **127 lines**
**Thresholds breached:** length 127 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-06 | 87 → 127 | first detection — #823: the jailed relaunch now re-wires a managed recipe, adopts the conversation and carries the renderer env, so claude and cursor relaunch through it. Not split: restructuring deferred to the sweep node (n20) |

## What would close it

Extract the managed re-wiring and the conversation adoption into helpers (`/code-restructuring` `extract_method`), sharing them with `resume_sandboxed_cli_harness`.
