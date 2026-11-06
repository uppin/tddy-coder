# complexity: reattach_recorded_main

**Location:** `packages/tddy-agent-launch/src/claude_cli_tmux.rs:80` — `reattach_recorded_main`
**Category:** complexity
**Detected:** 2026-11-06 — `/pr-wrap` on #823 (`#agent-tools` 17/19), fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run
**Metrics:** **76 lines**
**Thresholds breached:** length 76 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-06 | 70 → 76 | first detection — #823: re-attach resolves the recorded session type from the installed coding agents (`recorded_cli_session_type`), skips a record without one with a warning, and re-attaches every CLI agent through `cli_harness_tmux::reattach_recorded_harness`. Not split: restructuring deferred to the sweep node (n20) |

## What would close it

Extract the record read and the session-type resolution into one `fn recorded_reattach(...) -> Option<...>` so the function is the probe and the re-attach; rename the module (`claude_cli_tmux.rs` re-attaches every CLI agent now).
