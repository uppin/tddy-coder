# Declared tools — a tool is a YAML definition the sandbox lowers - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 1/12` (branch `feature/agent-tools/tool-defs`)

## Affected Features

- **Primary**: [Sandbox builder](../sandbox-builder.md) - a jail's executable surface comes from declared
  tool definitions, not from a plan builder hard-coding a lookup for one named binary
- **Related**: [Remote codebase mode § Workspace tool sandbox](../../daemon/remote-codebase-mode.md) - the
  workspace jail runs `Grep` through a declared `rg` tool and refuses to start when the host cannot
  satisfy it
- **Related**: [Session actions](../session-actions.md) - unchanged here. `tddy-actions` gains the tool
  layer that later nodes of `#agent-tools` hang actions and coding agents on

## Summary

A **tool** is a YAML definition, the Bazel-style "executable plus what it needs". It names how its
binary is resolved on the host, which other tools it **depends on**, and which host paths it needs
readable. `tddy-actions` loads the definitions from two sources:

- **built-ins** embedded in the binary;
- **overrides** in `<tddyhome>/tools/*.yaml`, where a file replaces a built-in with the same `id` and a
  new `id` adds a tool.

It resolves each definition against the host, together with the closure of its deps, and the sandbox
lowers that resolution into grants:

- the binary is exec-granted **as a file, not its directory**;
- its libraries and loader traversal are made readable;
- it is reachable on the jail's `PATH` through a jail-local `bin/` directory of links.

The first client is the one the design was decided for. The workspace tool jail's `Grep` runs `rg`
through a declared `rg` tool, which replaces the `host_ripgrep_grants` patch. A host without `rg`
refuses the jail at start-up and names the tool.

```yaml
# built-in: packages/tddy-actions/tools/rg.yaml
schema_version: 1
id: rg
summary: ripgrep — content search behind the Grep exec tool
binary:
  command: rg            # resolved on the daemon's PATH, symlinks canonicalized
  env: TDDY_RG_BINARY    # optional: an absolute path that wins over the PATH search
deps: []                 # other tool ids; their grants join this tool's
reads: []                # extra host paths the tool needs readable
```

## Background

`Grep` has never worked inside a Mac jail. The jail's PATH is `/usr/bin:/bin:/usr/sbin:/sbin`, and
Homebrew's `rg` is not on it. A temporary patch (`host_ripgrep_grants`) splices the host's `rg`
directory into the workspace jail's PATH and reads. Recorded in
[2026-09-26-grep-is-unreachable-inside-every-jail.md](../../../dev/todo/2026-09-26-grep-is-unreachable-inside-every-jail.md),
it has three problems:

- it widens the exec surface to a whole directory of unrelated binaries;
- it is silent when `rg` is missing;
- it is hard-coded for one binary.

The fix decided there is a declared tool list.

That same shape is what the `#agent-tools` stack needs for every coding agent. Claude, Cursor,
OpenCode and the others each become a tool definition, and the tool runtime, rather than per-agent
Rust, owns the sandbox setup. This node lays the foundation: the schema, the loader, deps, and host
resolution and lowering. It proves them on `rg`.

## Proposed Changes

### What's Changing

- **The `ToolDef` YAML schema (version 1)** with `id`, `summary`, `binary { command, env }`, `deps` and
  `reads`.
  - Unknown fields are refused.
  - Later nodes add their own sections: argv, env, home, files, network, transport and adapter.
- **A tool registry** with built-ins embedded from `packages/tddy-actions/tools/*.yaml` plus
  `<tddyhome>/tools/*.yaml` overrides. An override replaces a built-in **whole**, by `id`, and is never
  merged field by field.
- The registry refuses, naming the files and ids involved:
  - two files with one `id` in the same source;
  - a dep on an unknown id;
  - a dep cycle.
- **Host resolution** of a tool and its dep closure:
  - the binary resolves from the `env` override, or else from a `command` search of the daemon's PATH;
  - symlinks are canonicalized;
  - an unresolvable binary is an error naming the tool and what was searched.
- **Sandbox lowering** of a resolved tool set:
  - the binary is exec-granted as a literal file;
  - its dynamic dependencies, plus the nearest non-symlinked ancestor of each, become reads;
  - a jail-local `bin/` directory of links goes first on PATH.
- **The workspace tool jail** takes the `rg` tool from the registry.
  - When `rg` cannot be resolved, the jail fails to start with the tool named.
  - `host_ripgrep_grants` and its three `FIXME(grep-in-jail)` sites are deleted.

### What's Staying the Same

- The `ActionSpec`, `ProcessRuntime` and `SandboxRequest` APIs, and every other jail, including all
  coding-agent jails.
- A jail's PATH still starts from the system directories. What declared tools add, they add ahead of
  them.
- `Grep`'s behaviour inside the jail, apart from now working on a Mac and refusing loudly when `rg` is
  missing.

## Impact Analysis

### Technical Impact

- `tddy-actions` gains the tool modules: schema, registry and resolution, with no sandbox dependency.
- `tddy-sandbox-recipes` gains the lowering of resolved tools into `ReadSpec`, `SymlinkSpec` and PATH.
  It depends on `tddy-actions`, and the generic `tddy-sandbox` stays independent of it.
- `tddy-daemon-sandbox`'s workspace jail plan takes a resolved tool set.
- The daemon builds the registry once, from `<tddyhome>/tools`.

### User Impact

- `Grep` works in sandboxed and sandboxed-codebase sessions on macOS.
- When the host has no `rg`, a session's workspace jail refuses to start with a message naming `rg`,
  instead of failing every `Grep` call.
- An operator can point `rg` somewhere else with `<tddyhome>/tools/rg.yaml` or `TDDY_RG_BINARY`.

## Acceptance Criteria

- [ ] A sandboxed workspace jail runs `Grep` over its worktree through the declared `rg` and returns
  matches. Live on macOS, on the real Seatbelt backend.
- [ ] The jail's exec grants name the resolved `rg` binary file, not its directory, and no other
  binary from that directory is executable inside the jail.
- [ ] A host where `rg` cannot be resolved fails the workspace jail at start with an error naming the
  tool `rg` and what was searched.
- [ ] `<tddyhome>/tools/rg.yaml` replaces the built-in, and `TDDY_RG_BINARY` overrides the PATH search.
- [ ] A tool whose `deps` name another tool gets that tool's grants as well. An unknown dep and a dep
  cycle are each refused at registry load, naming the ids.
- [ ] An unknown field in a tool file is refused at load, naming the file and the field.

## Out of Scope

- **Argv, env, home, files, network, transport and adapter sections.** These belong to later
  `#agent-tools` nodes. A field is added by the node whose behaviour reads it.
- **Egress allowlists** (`#agent-tools 3/12`).
- **`tddy-build` tool targets** (`#agent-tools 4/12`).
- **Coding agents** (`#agent-tools 2/12` onwards).
- An in-process `Grep`. The todo weighs it, but `rg` is still the engine's implementation and the
  generic tool shape is the point.
