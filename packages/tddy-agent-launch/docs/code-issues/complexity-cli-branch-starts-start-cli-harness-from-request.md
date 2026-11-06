# complexity: start_cli_harness_from_request

**Location:** `packages/tddy-agent-launch/src/cli_branch_starts.rs:317` — `start_cli_harness_from_request`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **61 lines**
**Thresholds breached:** length 61 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 0 → 61 (new over the threshold; 41 lines changed) | #797: the request branch resolves the session type to a spec id, checks the spec's placements and coding agents before dispatching host or jail |
| 2026-11-05 | 60 (unchanged) | touched by #819 (`#agent-tools` 13/19) at the route call; measured fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run. Restructuring deferred to the #agent-tools sweep node (developer, 2026-11-04) |

## What would close it

Extract the spec resolution and placement check (`harness_from_spec::require_spec_placement` already holds half of it) into one helper.
