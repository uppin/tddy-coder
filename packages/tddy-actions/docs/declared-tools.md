# Declared tools (`tddy_actions::tool`)

Product contract: [Sandbox builder § Declared tools](../../../docs/ft/coder/sandbox-builder.md#declared-tools).

| Module | Holds |
|---|---|
| `tool/def.rs` | `ToolDef { schema_version, id, summary, binary: ToolBinary { command, env, target: Option<TargetBinary { target, path }> }, deps, reads, run: Option<RunSection>, extensions: BTreeMap<String, serde_yaml::Value>, network, home, files, secrets, policy }` — serde, `deny_unknown_fields` (`run`, `extensions` and the five sandbox sections default to empty); `ToolDef::parse(file, yaml)` checks `schema_version == 1` |
| `tool/run.rs` | `RunSection { transport, argv: Vec<ArgvItem>, env }`, `Transport::{Pty, Stdio}`, `ArgvItem::{Lit, When { if_, then, else_ }, Splice { splice }}` (untagged) |
| `tool/template.rs` | `TemplateParams` (`with`, `with_opt`, `with_flag`, `with_list`) and `render_argv(&RunSection, &ResolvedTool, &TemplateParams)` |
| `tool/extensions.rs` | `ExtensionOwner` — `name()` and `validate(tool_id, section)` |
| `tool/registry.rs` | `ToolRegistry` — `builtin()` (embedded `BUILTIN_TOOLS`), `from_embedded(sources)`, `load(builtin, overrides_dir)`, `load_with_owners(builtin, overrides_dir, owners)`, `get`, `origin`, `ids`, `resolve_closure(ids, host)`; `BUILTIN_SUPPORT_TOOLS` (`tddy-tools.yaml`) |
| `tool/resolve.rs` | `ToolHost` (env, `PATH` dirs, executable check, canonicalize, `target_output(&TargetBinary) -> Option<PathBuf>` — a default answering `None`), `ProcessToolHost`, `ResolvedTool { id, command, binary, reads }` |
| `tool/home.rs` | `HomeSection::{Persistent { key }, Session}` (tag `kind`, lowercase; default `Session`) |
| `tool/files.rs` | `FileItem::{SeedOnce { from_host, to }, Mirror { from_host, to }, Overlay { env, value }}` (tag `kind`, kebab-case). `from_host` is relative to the host user's home, `to` to the tool's; an overlay `value` may name `{home}` and `{tmp}` |
| `tool/secrets.rs` | `SecretItem { env, source: SecretItemSource, when: Option<RouteGuard> }`, `SecretItemSource::{HostEnv { var }, HostFile { path }}` (tag `kind`, kebab-case), `RouteGuard::{Unrouted, Routed}` |
| `tool/policy.rs` | `PolicySection { jit, fork, exec_paths, sysctl_read }`, every field defaulting to no allow |
| `tool/error.rs` | `ToolError` — `Parse`, `DuplicateId`, `UnknownDep`, `DepCycle`, `UnknownTool`, `Unresolvable`, `DuplicateCommand`, `UnknownExtension`, `ExtensionRejected`, `UnboundPlaceholder`, `Io` |
| `tools/*.yaml` | built-in definitions (`rg.yaml`, and the support tool `tddy-tools.yaml`), embedded with `include_str!` |

## Rules

- **Load** validates the merged set: an override replaces a built-in whole by `id`; a duplicate id
  within one source, an unknown dep and a dep cycle are refused. A missing overrides directory adds
  nothing; one that cannot be read is `Io`; an override file that cannot be read is `Parse`.
  Override files are read in name order, so a duplicate is always reported against the same pair.
- **Resolve** walks deps depth-first, each tool once, deps before dependents. The first tool the host
  cannot satisfy fails the whole closure. Two tools in one closure with the same `binary.command` are
  `DuplicateCommand`: both would claim one link in the jail's `bin/`.
- **Binary source** order: a `binary.target` (another `tddy-build` target's output) first — resolved by `ToolHost::target_output`, which only `tddy-build`'s run host answers, so such a tool is `Unresolvable` anywhere else; a target output that is not executable is refused — then the `env` override, then the first `PATH` directory holding `command`.
- **`ProcessToolHost`** drops relative and empty `PATH` entries (they name the daemon's working
  directory). An unset `PATH` is an empty search, reported as `PATH (empty)`, and refuses the tool.
- **Render** emits one argv entry per literal, with `{name}` replaced and `{binary}` bound to the
  resolved binary. A brace pair that does not enclose an identifier stays literal, and a substituted
  value is never re-scanned. A `When` renders `then` when its parameter is bound to a non-empty
  value and `else` otherwise, recursively. A `Splice` expands its list. An unbound name is
  `UnboundPlaceholder`.
- **Extensions** are checked by `load_with_owners` after the merge, so an override's section is what
  is validated. An unclaimed key is `UnknownExtension` and a refused section is `ExtensionRejected`,
  both naming the tool's file. `from_embedded` validates ids and deps only.
- **`network:`** (`tool/network.rs`): `NetworkSection { egress: Egress::{None, Any, Hosts}, oauth_inbound }`,
  absent means `none`. `HostPattern::parse` validates `host[:port]` / `*.domain[:port]` (port default
  443, lowercased); a bad pattern refuses the load naming the tool. `Egress` has hand-written serde
  because serde_yaml 0.9 reads a derived enum only as a YAML tag, never as `{ hosts: [...] }`.
  `ResolvedTool.network` carries the section for `tool_grants::lower_network`.
- **`home:`, `files:`, `secrets:`, `policy:`** are plain serde types with no behaviour here; they are
  read from the `ToolDef` (not from `ResolvedTool`) and lowered by `tool_grants::{lower_home,
  lower_files, lower_secrets, lower_policy}`. A secret's `host-env` source is read through the same
  `ToolHost` the tool resolved against. Product contract:
  [Sandbox builder § Jail sections](../../../docs/ft/coder/sandbox-builder.md#jail-sections).
- The crate has no sandbox dependency. Lowering a resolved set into grants is
  `tddy-sandbox-recipes::tool_grants`.

Tests: `tests/tool_registry_acceptance.rs`, `tests/tool_resolve_unit.rs` (fixture `ToolHost` in
`tests/support/`), `tool::template::tests` (the template grammar).
