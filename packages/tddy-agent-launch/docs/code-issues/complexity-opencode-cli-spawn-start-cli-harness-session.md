# complexity: start_cli_harness_session

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn.rs:133` — `start_cli_harness_session`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **191 lines**
**Thresholds breached:** length 191 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 175 → 191 | #797: the host OpenCode/Muse/Warp start now renders its argv from the spec (`harness_from_spec`) and installs Warp's resume-token `on_exit` hook; the old `CliHarness` argv path it replaces stays until the legacy deletion (`docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md`) |

## What would close it

Extract the worktree/route provisioning and the spec render into helpers through `/code-restructuring` (`extract_method`), with parameter structs so none passes seven parameters. Deleting the legacy `CliHarness` path first shrinks the function.
