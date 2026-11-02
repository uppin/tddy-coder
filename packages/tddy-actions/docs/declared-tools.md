# Declared tools (`tddy_actions::tool`)

Product contract: [Sandbox builder § Declared tools](../../../docs/ft/coder/sandbox-builder.md#declared-tools).

| Module | Holds |
|---|---|
| `tool/def.rs` | `ToolDef { schema_version, id, summary, binary: ToolBinary { command, env }, deps, reads }` — serde, `deny_unknown_fields`; `ToolDef::parse(file, yaml)` checks `schema_version == 1` |
| `tool/registry.rs` | `ToolRegistry` — `builtin()` (embedded `BUILTIN_TOOLS`), `load(builtin, overrides_dir)`, `get`, `origin`, `ids`, `resolve_closure(ids, host)` |
| `tool/resolve.rs` | `ToolHost` (env, `PATH` dirs, executable check, canonicalize), `ProcessToolHost`, `ResolvedTool { id, command, binary, reads }` |
| `tool/error.rs` | `ToolError` — `Parse`, `DuplicateId`, `UnknownDep`, `DepCycle`, `UnknownTool`, `Unresolvable`, `DuplicateCommand`, `Io` |
| `tools/*.yaml` | built-in definitions (`rg.yaml`), embedded with `include_str!` |

## Rules

- **Load** validates the merged set: an override replaces a built-in whole by `id`; a duplicate id
  within one source, an unknown dep and a dep cycle are refused. A missing overrides directory adds
  nothing; one that cannot be read is `Io`; an override file that cannot be read is `Parse`.
  Override files are read in name order, so a duplicate is always reported against the same pair.
- **Resolve** walks deps depth-first, each tool once, deps before dependents. The first tool the host
  cannot satisfy fails the whole closure. Two tools in one closure with the same `binary.command` are
  `DuplicateCommand`: both would claim one link in the jail's `bin/`.
- **`ProcessToolHost`** drops relative and empty `PATH` entries (they name the daemon's working
  directory). An unset `PATH` is an empty search, reported as `PATH (empty)`, and refuses the tool.
- The crate has no sandbox dependency. Lowering a resolved set into grants is
  `tddy-sandbox-recipes::tool_grants`.

Tests: `tests/tool_registry_acceptance.rs`, `tests/tool_resolve_unit.rs` (fixture `ToolHost` in
`tests/support/`).
