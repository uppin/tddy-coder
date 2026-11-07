# complexity: start_cli_harness_session

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn.rs:133` — `start_cli_harness_session`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **301 lines**
**Thresholds breached:** length 191 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — partially fixed 2026-11-06 by #823 (303 → 301), still over 60 — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 175 → 191 | #797: the host OpenCode/Muse/Warp start now renders its argv from the spec (`harness_from_spec`) and installs Warp's resume-token `on_exit` hook; the old `CliHarness` argv path it replaces stays until the legacy deletion (`docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md`) |
| 2026-11-05 | 197 → 200 | **worse by 3** in #819 (`#agent-tools` 13/19): `start_cli_route` no longer takes `kind.route` (−1), and `routed_launch_model` is handed the route's arguments (`route.as_ref().map(|route| route.args.as_slice())`, wrapped onto four lines, +4) so a route that names the model leaves the argv without one; measured fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run. Restructuring deferred to the #agent-tools sweep node (developer, 2026-11-04) |
| 2026-11-05 | 200 → 303 | **worse by 103** in #820 (`#agent-tools` 14/19): the generic start gained the spec-keyed steps claude's and cursor's needs ask of it — the role refusal, the session wiring (`cli_agent_steps::wire_session_agent`: hooks, managed workflow, notify, role), the role recorded before the spawn, the conversation (`start_conversation`), the slots through `render_host_launch_slots`, the ssh slot write, the node launcher, the managed workflow attached after the spawn and the extra `.session.yaml` fields; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap. The steps live in `cli_agent_steps`, but their call sites and the struct literals passing their inputs are inline. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 303 → 301 | **better by 2** in #823 (`#agent-tools` 17/19): the participant metadata is built in one call; the renderer env and the session pointer (write + exit removal) added lines back; measured fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |
| 2026-11-07 | 300 → 300 | **unchanged** in #829 (`#agent-tools` 21/28), which touched it: the session-pointer call takes a `PointerHost` bound on its own line; fn line to closing brace on `origin/master` (`d3fe1abea`) and HEAD (300 by this count; 301 above) |

## What would close it

Extract the worktree/route provisioning and the spec render into helpers through `/code-restructuring` (`extract_method`), with parameter structs so none passes seven parameters. Deleting the legacy `CliHarness` path first shrinks the function.
