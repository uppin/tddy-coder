# complexity: resume_cli_harness_session

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn.rs:536` — `resume_cli_harness_session`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **97 lines**
**Thresholds breached:** length 74 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-05 by #820 (74 → 97) — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 61 → 74 | #797: the host resume renders the spec's resume argv and wires the resume adapter's `on_exit` hook |
| 2026-11-05 | 74 (unchanged) | touched by #819 (`#agent-tools` 13/19): `resume_cli_route` reads the agent from the spec; measured fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run. Restructuring deferred to the #agent-tools sweep node (developer, 2026-11-04) |
| 2026-11-05 | 74 → 97 | **worse by 23** in #820: the resume re-wires the session (`resumed_session_wiring`), binds the resumed conversation and the node launcher and writes the slot files on an ssh target; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |

## What would close it

Share the spec render and hook wiring with `start_cli_harness_session` (one extracted helper), which takes both under 60.
