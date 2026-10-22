# 2026-10-22 — A runner spawn carries its provider route; the stdio acceptance fixture stays alive and cleans up

**Type:** Feature + Fix (test fixture)

- **`SandboxRunnerSpawn.route: Option<HarnessRoute>`** is passed into the recipe plan
  (`RunnerPlanRequest.route`). There, the route's secrets become jail secrets (`TDDY_SECRET_<NAME>`), its
  environment joins the jail's, and its files are written owner-only into the scratch.
  `build_workspace_tool_plan` and `build_action_runner_plan` pass `None`: neither jail hosts an agent,
  so neither takes a route.
- **Fix — `sandbox_session_stdio_acceptance`**: the fake agent is a script that runs
  `exec /bin/sleep 600`. Plain `/bin/sleep` handed claude's argv rejected it and exited before the tool
  call, and the host stops collecting frames after `SessionEnded`. The jail's process group is now
  terminated by a drop guard, so a failing assertion no longer leaves a runner (and its ten-minute
  sleep) behind. `sandbox_runner_stdio_acceptance` skips the jail's empty attach acknowledgement.

The fixture defect is pre-existing, from #481. It was fixed here at the developer's request, together
with the runner's attach and channel-release fixes
([tddy-sandbox-runner changeset](../../../tddy-sandbox-runner/docs/changesets/2026-10-22-provider-route-sandboxed-routes.md)).

PR [#670](https://github.com/uppin/tddy-coder/pull/670) (`#provider-route` 9/9).
