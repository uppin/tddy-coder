# complexity: LaunchSessions::launch_jailed_cli_agent

**Location:** `packages/tddy-agent-launch/src/cli_harness_jail.rs:237` — `LaunchSessions::launch_jailed_cli_agent`
**Category:** complexity
**Detected:** 2026-11-06 — `/pr-wrap` on #823 (`#agent-tools` 17/19), fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run
**Metrics:** **82 lines**
**Thresholds breached:** length 82 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-06 | 41 → 82 | first detection — #823: the generic jail gained claude's and cursor's slots — the tool-server registration and its jail home, the tool lists and permission prompt (`JailToolset`), the managed prompt as a flag or a rules file, the passthrough args. Not split: restructuring deferred to the sweep node (n20) |

## What would close it

Move the home and slot rendering into `JailToolset` (or a `JailSlots::render`) so the method only assembles `JailSpec` and `JailedSession` (`/code-restructuring` `extract_method`).
