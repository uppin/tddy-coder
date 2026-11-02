# 2026-11-02 — BUILD.yaml tools are tool definitions; build actions can run hermetically

**Type:** Feature

`#agent-tools 4/12` ([PR #793](https://github.com/uppin/tddy-coder/pull/793)). Packages:
`tddy-build`, `tddy-actions`, `tddy-daemon-sandbox`, `tddy-bsp`.

- **Tool targets are `ToolDef`s.** A `type: tool` target is an inline tool definition or `ref:` a
  registry tool (built-ins + `<tddyhome>/tools`). The legacy `{bin_dir, commands}` shape is refused
  naming the target; `executor::tool_path` and `builtin::{LegacyToolTarget, tool_bin_dir}` are gone,
  and `examples/pipeline/app/BUILD.yaml` is migrated.
- **Built, then used.** `ToolBinary.target: Option<TargetBinary { target, path }>` and
  `ToolHost::target_output` (default `None`) let a tool's binary be another target's output; that
  target orders before every action naming the tool, and a cycle through it is reported.
- **Resolution.** `tool_dep_ids` resolve through `ToolRegistry::resolve_closure` in
  `executor::tool_run`; the closure's binary directories go on `PATH`.
- **Hermetic actions.** `BuildAction.sandbox` (proto field 10), `ExecuteOptions.hermetic`, and
  `tddy-tools build --hermetic` (refused when relayed over `TDDY_SOCKET`). The engine spawns them
  through the `tddy_build::ActionSandbox` port; `tddy_daemon_sandbox::build_action_sandbox::DaemonActionSandbox`
  implements it with read-only inputs, outputs as the only writable paths, `lower_tools` and
  `lower_network` of the closure.
- **Cache.** `compute_run_cache_key` covers the resolved tool binaries' fingerprints and the
  action's hermeticity.

Closes the *Hermetic sandboxing* item of [2026-06-16-tddy-build.md](../todo/2026-06-16-tddy-build.md).
Deferred: [Linux write/exec confinement](../todo/2026-11-02-hermetic-build-actions-unconfined-on-linux.md)
(two jail tests macOS-only, `FIXME(fs-confinement)`),
[executable outputs](../todo/2026-11-02-hermetic-build-action-outputs-are-executable.md),
[egress shim](../todo/2026-11-02-hermetic-build-actions-have-no-egress-shim.md),
[dependency failure does not stop the build](../todo/2026-11-02-build-dependency-failure-does-not-stop-the-build.md),
[plugin-lowered tool deps ordering](../todo/2026-11-02-plugin-lowered-tool-deps-get-no-build-order.md).

Docs: [tddy-build](../../ft/build/tddy-build.md), [tddy-build architecture](../../../packages/tddy-build/docs/architecture.md),
[declared tools](../../../packages/tddy-actions/docs/declared-tools.md),
[tddy-daemon-sandbox services](../../../packages/tddy-daemon-sandbox/docs/task-and-action-services.md).
