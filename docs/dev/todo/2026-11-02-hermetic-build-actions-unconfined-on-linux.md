# 2026-11-02 — Hermetic build actions are not write- or exec-confined on Linux

**Category:** Deferred from `#agent-tools` 4/12 (#793)
**Source:** `/validate-changes` on `feature/agent-tools/build-tools`

## What is missing

`DaemonActionSandbox` (`packages/tddy-daemon-sandbox/src/build_action_sandbox.rs`) runs a
`sandbox: true` / `--hermetic` action through `spawn_sandbox_plan`. On macOS that is Seatbelt, which
refuses writes outside the declared outputs and exec outside the tool closure. On Linux it is the
cgroups jail, which still shares the host filesystem root (`FIXME(fs-confinement)` on
`tddy_sandbox_cgroups::spawn_plan`): reads become read-only bind mounts, but nothing stops a write to
an undeclared path or an exec of a binary outside the closure. Network (netns) and resource limits
are enforced.

So two acceptance tests in `packages/tddy-daemon-sandbox/tests/hermetic_build_action_acceptance.rs`
are `#[cfg(target_os = "macos")]`-gated, each with a `FIXME(fs-confinement)`:
`a_hermetic_action_writing_outside_its_outputs_fails_naming_the_path` and
`a_hermetic_action_execing_a_binary_outside_its_tool_closure_fails_naming_it`. CI (ubuntu) does not
run them.

## Why it was deferred

The gap is the cgroups backend's, not the build integration's: #793 only lowers an action to a
`SandboxPlan`. Closing it is the `pivot_root` minimal-root work the backend already names.

## What would close it

The cgroups jail pivots into a minimal root built from the plan's reads and mounts. Then drop the two
`cfg` gates and the `allow(dead_code)` on `ARepo::with_script`.

On CI the remaining Linux-run hermetic tests also self-skip, because the runners forbid unprivileged
user namespaces: see [2026-11-02-ci-runs-no-linux-jail.md](2026-11-02-ci-runs-no-linux-jail.md).
