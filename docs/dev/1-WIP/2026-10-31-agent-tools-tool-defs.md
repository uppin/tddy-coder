# Changeset: Declared tools — a tool is a YAML definition the sandbox lowers

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — three crates (`tddy-actions`, `tddy-sandbox-recipes`, `tddy-daemon-sandbox`) plus daemon wiring; a new YAML contract that eleven later nodes build on; a live Seatbelt seam
**Stack:** `#agent-tools 1/12` — branch `feature/agent-tools/tool-defs`, base `master`
**PRD:** [PRD-2026-10-31-declared-tools.md](../../ft/coder/1-WIP/PRD-2026-10-31-declared-tools.md)

## Initial Discovery

[2026-10-31-agent-tools-tool-defs-initial-discovery.md](./2026-10-31-agent-tools-tool-defs-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-09-26-grep-is-unreachable-inside-every-jail.md](../todo/2026-09-26-grep-is-unreachable-inside-every-jail.md) | ✅ **RESOLVED HERE.** This node builds the declared tool list (`ToolSpec[]`) that the entry decided on. It grants the binary rather than its directory and refuses at jail start when the host cannot satisfy a tool, and it deletes `host_ripgrep_grants` along with its three `FIXME(grep-in-jail)` sites |
| `packages/tddy-sandbox/docs/code-issues/oversized-file-builder.md` | ⚠ During — the lowering lives in `tddy-sandbox-recipes` (new module `tool_grants.rs`), and `tddy-sandbox/src/builder.rs` gains no lines |
| `tddy-actions`, `tddy-sandbox-recipes` | ℹ Neither has a `docs/code-issues/` directory. Not analyzed is not the same as clean, so both new modules stay inside the file and function budgets |

## Affected Packages

- [ ] [`tddy-actions`](../../../packages/tddy-actions/README.md): `tool` module — the `ToolDef` schema, the `ToolRegistry` (embedded built-ins plus `<tddyhome>/tools` overrides) and host resolution of the dep closure; built-in `tools/rg.yaml`
- [ ] [`tddy-sandbox-recipes`](../../../packages/tddy-sandbox-recipes/README.md): `tool_grants` — lowers a resolved tool set to `ReadSpec`, `SymlinkSpec` and PATH; depends on `tddy-actions`
- [ ] [`tddy-daemon-sandbox`](../../../packages/tddy-daemon-sandbox/README.md): the workspace jail plan takes the registry's `rg`; `host_ripgrep_grants` is deleted
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): builds the `ToolRegistry` once at start-up and hands it to the workspace-jail provisioner

## Responsibility

- `tddy_actions::tool::ToolDef` — `{ schema_version: u32, id: String, summary: String, binary: ToolBinary { command: String, env: Option<String> }, deps: Vec<String>, reads: Vec<PathBuf> }`, with serde and `deny_unknown_fields`.
- `tddy_actions::tool::ToolRegistry::{builtin() -> Self, load(builtin, overrides_dir: &Path) -> Result<Self, ToolError>, get(&str) -> Option<&ToolDef>, ids()}`.
  - An override replaces a built-in whole, by `id`.
  - Load refuses an unknown field, a duplicate id within one source, an unknown dep and a dep cycle, each naming the ids and files involved.
- `tddy_actions::tool::ToolHost` — a trait for the environment a tool resolves against (`env_var`, `path_search`, `canonicalize`). Production uses the daemon's process; tests inject a fixture host.
- `tddy_actions::tool::ResolvedTool { id, binary: PathBuf, reads: Vec<PathBuf> }` and `ToolRegistry::resolve_closure(&[&str], &dyn ToolHost) -> Result<Vec<ResolvedTool>, ToolError>`.
  - Resolution order is `env` override, then `command` on PATH, then canonicalize.
  - `ToolError::Unresolvable { tool, searched }` names the tool and what was searched.
- `tddy_sandbox_recipes::tool_grants::{ToolGrants { reads: Vec<ReadSpec>, symlinks: Vec<SymlinkSpec>, path_prefix: PathBuf }, lower_tools(&[ResolvedTool], jail_bin_dir: &Path) -> ToolGrants}`.
  - The binary is a literal executable read.
  - Each `otool -L` dependency is readable, and so is the nearest non-symlinked ancestor of each.
  - `jail_bin_dir/<command>` links to the resolved binary.
- `tddy_daemon_sandbox::workspace_tool_sandbox::WorkspaceToolPlanRequest.tools: Vec<ResolvedTool>`.
  - `JailedWorkspaceSandboxProvisioner` resolves `["rg"]` from the registry at jail start, and refuses the start on `ToolError`.
- `packages/tddy-actions/tools/rg.yaml`, the first built-in tool.

## Boundaries

- No argv, env, home, files, network, transport or adapter sections. Each later `#agent-tools` node adds the section its own behaviour reads.
- No change to `ActionSpec`, `ProcessRuntime`, `SandboxRequest`, `ActionService` or any coding-agent jail.
- `tddy-sandbox` itself does not depend on `tddy-actions`. The dependency runs recipes → actions only.
- Does not touch `tddy-build`'s `tool` target (`#agent-tools 4/12`).
- No in-process `Grep`.

## Dependencies

None — this is the root node. It depends on `master` only.

## Draft PR contract

The first push publishes:

- the `tool` module types and signatures above (`ToolDef`, `ToolBinary`, `ToolRegistry`, `ToolHost`, `ResolvedTool`, `ToolError`), with `load`, `resolve_closure` and `lower_tools` bodies as `// TODO(tool-defs): implement`;
- `tools/rg.yaml`;
- the new `tools` field on `WorkspaceToolPlanRequest`;
- the failing tests below.

`host_ripgrep_grants` stays until green replaces it.

## Green wave

**Wave:** 1 of 7
**Greenable independently:** yes — it has no parent, and every test uses a fixture `ToolHost` or the real host's `rg`
**Concurrent with:** none (wave 1 has one node)
**Blocks:** every other node:

- #2 claude-host's specs load through this registry;
- #3 egress-allowlist adds a schema section to `ToolDef`;
- #4 build-tools resolves `tool` targets through it;
- the rest are blocked transitively.

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- **`tddy-actions`** has `ActionSpec`, which carries serde but no file format, and a hard-coded `ActionCatalog` of seven kind strings. Nothing declares a tool.
- **`tddy-build`** has a `tool` target `{bin_dir, commands}`, and only `bin_dir` is read.
- **Workspace jail.** `build_workspace_tool_plan` (`tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`) calls `host_ripgrep_grants()`. That function runs `which rg`, canonicalizes the result, then:
  - prepends the binary's **directory** to the jail's PATH;
  - adds `binary_exec_reads`, which makes the directory executable, plus traversal ancestors.
- When `rg` is missing it grants nothing, and every `Grep` call fails with `spawn failed`.
- `tests/ripgrep_in_jail_seatbelt.rs` proves the patch on a Mac.
- The jail PATH is `/usr/bin:/bin:/usr/sbin:/sbin` (`tddy-sandbox/src/runner_env.rs:12`).

## State B

- **At start-up**, the daemon loads `ToolRegistry::load(builtin(), <tddyhome>/tools)`. A bad tool file fails the daemon's start, naming the file.
- **At jail start**, the workspace provisioner resolves `["rg"]` and its closure against the daemon's process host.
  - On error, the jail start fails with `tool rg is unavailable: searched TDDY_RG_BINARY (unset), PATH (/usr/bin:…)`.
- **The plan builder** calls `lower_tools`:
  - `<scratch>/bin/rg` links to the canonical `rg`;
  - `<scratch>/bin` goes first on PATH;
  - the reads grant the binary file as a literal executable, plus its libraries and traversal.
- **`host_ripgrep_grants`** is gone.

## Delta

### New
- `packages/tddy-actions/src/tool/{mod.rs, def.rs, registry.rs, resolve.rs}`, plus `packages/tddy-actions/tools/rg.yaml`. `serde_yaml` becomes a dependency, and the crate's dev-dependencies already include `tempfile`.
- `packages/tddy-sandbox-recipes/src/tool_grants.rs`.

### Modified
- `tddy-sandbox-recipes/Cargo.toml` gains `tddy-actions`.
- `workspace_tool_sandbox.rs`: the request field and the provisioner's resolution. `host_ripgrep_grants`, `RipgrepGrants` and `binary_dependency_traversal_dirs` move into `tool_grants` in generic form.
- `tddy-daemon` runtime: builds the registry and hands it to `JailedWorkspaceSandboxProvisioner`.
- `tests/ripgrep_in_jail_seatbelt.rs` resolves `rg` through the registry.
- Docs, at wrap: `docs/ft/coder/sandbox-builder.md` § Declared tools; `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox; `packages/tddy-actions/docs/`.

## Testing Strategy

### Acceptance tests

**`tddy-daemon-sandbox`**: `packages/tddy-daemon-sandbox/tests/declared_tools_workspace_jail_acceptance.rs`

- [ ] `a_workspace_jail_built_from_the_declared_rg_tool_runs_rg_over_its_worktree_under_seatbelt` (macOS only, real `sandbox-exec`)
- [ ] `the_jail_grants_the_resolved_rg_file_and_no_sibling_binary_in_its_directory_is_executable` (macOS only, real `sandbox-exec`, running a sibling probe binary)
- [ ] `a_host_that_cannot_resolve_rg_refuses_the_workspace_jail_at_start_naming_the_tool_and_the_search`
- [ ] `an_override_in_tddyhome_tools_replaces_the_builtin_rg_and_the_jail_uses_it`

**`tddy-actions`**: `packages/tddy-actions/tests/tool_registry_acceptance.rs`

- [ ] `the_builtin_registry_offers_rg`
- [ ] `an_override_file_replaces_a_builtin_whole_by_id`
- [ ] `an_override_with_a_new_id_adds_a_tool`
- [ ] `an_unknown_field_is_refused_naming_the_file_and_the_field`
- [ ] `a_dep_on_an_unknown_tool_is_refused_naming_both_ids`
- [ ] `a_dep_cycle_is_refused_naming_the_cycle`
- [ ] `two_override_files_with_one_id_are_refused_naming_both_files`

### Unit / integration tests

- [ ] `tddy-actions` `tool::resolve`:
  - the `env` override wins over the PATH search;
  - a PATH hit through a symlink canonicalizes;
  - the closure includes deps transitively and once each;
  - an unresolvable binary yields `Unresolvable` naming what was searched.
- [ ] `tddy-sandbox-recipes` `tests/tool_grants_unit.rs`:
  - the binary is a literal executable read, not a subpath of its directory;
  - a link `bin/<command>` points at the binary;
  - `path_prefix` is the jail bin dir;
  - dep tools' grants are included.
- [ ] `tddy-daemon-sandbox` `tests/workspace_tool_sandbox_plan_unit.rs` (extended): the plan's PATH starts with the jail bin dir, and no read names the binary's directory as executable.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Schema, registry, resolution | Unit and integration over fixture files plus a fixture `ToolHost` | Deterministic and host-independent |
| Lowering | Unit over `ResolvedTool` fixtures | Pure data in, pure data out |
| `rg` actually runs in a jail | Live `sandbox-exec` on macOS | The defect only exists under the real Seatbelt loader. A plan-shape assertion is what let it survive before |

## Decisions & Trade-offs

- **A tool is an executable plus what it needs, and an action is one invocation of a tool** — Bazel's split. `ActionSpec` is untouched here. Later nodes let an invocation name a tool.
- **Override is replace-whole by id, never a field merge.** A partial merge would make the effective definition invisible in any one file.
- **The binary, not its directory.** A jail-local link directory on PATH, plus a literal exec grant, keeps the explicit-allow-list claim. That is what the todo asked for.
- **Resolution is at jail start, not daemon start.** An `rg` installed after the daemon starts is picked up, and a missing one refuses only the jails that need it.
- **Lowering lives in `tddy-sandbox-recipes`**, because the generic `tddy-sandbox` must not learn the tool vocabulary.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-tool-defs-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
