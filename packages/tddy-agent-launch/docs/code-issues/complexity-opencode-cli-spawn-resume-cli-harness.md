# complexity: LaunchSessions::resume_cli_harness

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn.rs:438` — `LaunchSessions::resume_cli_harness`
**Category:** complexity
**Detected:** 2026-11-06 — `/pr-wrap` on #823 (`#agent-tools` 17/19), fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run
**Metrics:** **88 lines**
**Thresholds breached:** length 88 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-06 | 65 → 88 | first detection — #823: a split session's wiring is re-derived first (`resume_split_wiring`), and its context directory and empty commit identity chosen, now that every claude-cli session resumes here. Not split: restructuring deferred to the sweep node (n20) |

## What would close it

Move the split half (`resume_split_wiring`, the worktree choice, the identity choice) into one `fn resumed_placement(...) -> ResumedPlacement` beside it (`/code-restructuring` `extract_method`).
