# complexity: start_cli_harness_from_request

**Location:** `packages/tddy-agent-launch/src/cli_branch_starts.rs:144` — `start_cli_harness_from_request`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **92 lines**
**Thresholds breached:** length 61 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — partially fixed by #924 (`#remote-shell` 8/11): restored to below its before-the-stack length (84); still over 60 — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 0 → 61 (new over the threshold; 41 lines changed) | #797: the request branch resolves the session type to a spec id, checks the spec's placements and coding agents before dispatching host or jail |
| 2026-11-05 | 60 (unchanged) | touched by #819 (`#agent-tools` 13/19) at the route call; measured fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run. Restructuring deferred to the #agent-tools sweep node (developer, 2026-11-04) |
| 2026-11-05 | 60 → 89 | **worse by 29** in #820: the request branch builds the managed wiring (`managed_recipe_for`, `managed_spawn_handlers`, `ManagedWiring`) and the start role, and passes the permission mode, ssh host and managed flags on to `start_cli_harness_session`; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 89 → 90 | **worse by 1** in #823 (`#agent-tools` 17/19): moved up the file as the claude and cursor request starts above it were deleted; measured fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |
| 2026-11-07 | 89 → 92 | **worse by 3** in #834 (`#agent-tools` 26/28): the request branch resolves the session user's agent-tool socket before anything is cut (`agent_tool_socket_of`, refusing an unlisted user) and hands it to the role wiring and to `CliSessionStart`; measured fn line to closing brace on `origin/feature/agent-tools/split-remote-env` (`cd9026d9d`) and HEAD; no coverage run (89 at the base by this count; 90 above) |
| 2026-11-16 | 98 → 100 | #921 (`#remote-shell` 7/7): the two branch starts hand the start its caller subject and Coder credentials. Touched, not closed |
| 2026-11-16 | 100 → 84 | #924 (`#remote-shell` 8/11) sweep: restored to below its before-the-stack length (94). Still over 60, so this record stays open on the remainder |
| 2026-11-21 | 89 → 83 | **better by 6** in #980 (`#orchestra` 2/9): `reports_to` added, the stack-parent resolution moved to `spawn_work_queue_parent_of`; brace-matched fn line to closing brace on `origin/feature/orchestra/path-reach` (`c4dace05c`) and HEAD. Still over 60 |

## What would close it

Extract the spec resolution and placement check (`harness_from_spec::require_spec_placement` already holds half of it) into one helper.
