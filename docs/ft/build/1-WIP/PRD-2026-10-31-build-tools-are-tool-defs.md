# BUILD.yaml tools are tool definitions, and build actions can run hermetically - PRD

**Date**: 2026-10-31
**PRD Type**: Enhancement
**Stack**: `#agent-tools 4/12` (branch `feature/agent-tools/build-tools`)

## Affected Features

- **Primary**: [tddy-build](../tddy-build.md). The built-in `tool` target becomes a tool definition, and actions can run inside a jail.
- **Related**: [Sandbox builder](../../coder/sandbox-builder.md). A `tddy-build` target can back a tool. This is the "names a host binary or a tddy-build target" half of the declared-tool design.

## Summary

Three changes:

1. **A BUILD.yaml `type: tool` target is a `ToolDef`.** Its config is either an inline definition or `ref: <tool id>`, which names a registry tool. Its binary can also be another target's output (`binary: { target: "//tools:stamp", path: bin/stamp }`), so a tool can be **built and then used**.
2. **`tool_dep_ids` on an action resolve through the tool registry.** The action gets the tool closure's binaries on PATH, exactly as a jail would, and the directory-only `bin_dir` is gone.
3. **An action can run hermetically.** `sandbox: true` on an action, or `tddy build --hermetic` for all of them, lowers the action through the same tool runtime:
   - the action's `inputs` are read-only mounts;
   - its declared outputs are the only writable paths;
   - its tools come from `tool_dep_ids`;
   - `network` is `none` unless a tool declares otherwise.

   An action that writes an undeclared path or execs an undeclared binary fails, and the error names the path.

```yaml
targets:
  - id: "tools:stamp"
    config: { type: tool, binary: { target: "stamp:bin", path: target/debug/stamp }, reads: [] }
  - id: "app:build"
    actions:
      - id: package
        type: command
        command: ["sh", "-c", "stamp < lib.txt > app.txt"]
        tool_dep_ids: ["tools:stamp"]
        sandbox: true
```

## Background

`tddy-build`'s `tool` type has `{bin_dir, commands}`. Only `bin_dir` is read, `ActionType::TOOL` is declared but unimplemented, and "hermetic sandboxing" is listed as not done ([2026-06-16-tddy-build.md](../../../dev/todo/2026-06-16-tddy-build.md)). The declared-tool layer from `#agent-tools 1/12` is the same Bazel-shaped concept, so the developer asked for the two to be unified in this stack. That gives build actions and coding agents one tool model and one sandbox lowering.

## Proposed Changes

### What's Changing

- `builtin::ToolTarget` becomes `ToolDef` (inline) or a `ref`.
  - The old `{bin_dir, commands}` shape is refused, with a migration message naming the target.
  - The repo's own BUILD.yaml files are migrated. The only existing user is `examples/pipeline/app/BUILD.yaml`.
- A tool target's binary may be another target's declared output. That target becomes a build-order predecessor of every action using the tool.
- `executor::tool_path` is replaced by tool resolution through `tddy-actions`.
- A `BuildAction` gains `sandbox: bool`, and `tddy build` gains `--hermetic`.
- A sandboxed action is spawned through an **`ActionSandbox` port** that the executor is handed. That keeps `tddy-build` free of sandbox crates. The production implementation lives in `tddy-daemon-sandbox`'s action runtime and is wired by `tddy-bsp` / `tddy-tools`. The plan is lowered from:
  - its inputs as read-only mounts;
  - its outputs as writable paths;
  - its tool closure.
- The cache key includes the resolved tool binaries' fingerprints, so changing a tool invalidates its users.

### What's Staying the Same

- Non-hermetic actions run as today, through `ProcessRuntime`.
- Plugin crates, the manifest format apart from `type: tool`, and discovery.

## Acceptance Criteria

- [ ] An action naming a tool target whose binary is built by another target builds that target first, then runs the action with the tool on PATH.
- [ ] A `ref:` tool target resolves from the registry, including `<tddyhome>/tools` overrides.
- [ ] A hermetic action that writes only its declared output succeeds.
  - The same action writing outside its outputs fails, naming the path. Live on macOS Seatbelt and Linux cgroups (the latter is CI-only).
- [ ] A hermetic action that execs a binary outside its tool closure fails, naming it.
- [ ] A hermetic action with no network-declaring tool has no network.
- [ ] Changing a tool's binary invalidates the cache of every action using it.
- [ ] The legacy `{bin_dir, commands}` tool shape is refused with a migration message.

## Out of Scope

- `ActionType::COPY`.
- Remote execution.
- Making hermetic the default.
