# Changeset: BUILD.yaml tools are tool definitions, and build actions can run hermetically

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — `tddy-build`, `tddy-actions`, `tddy-daemon-sandbox`, `tddy-bsp`/`tddy-tools`; a manifest format change (breaking for `type: tool`); cache-key change; live jail tests
**Stack:** `#agent-tools 4/12` — branch `feature/agent-tools/build-tools`, base `feature/agent-tools/egress-allowlist`
**PR:** [#793](https://github.com/uppin/tddy-coder/pull/793)
**PRD:** [PRD-2026-10-31-build-tools-are-tool-defs.md](../../ft/build/1-WIP/PRD-2026-10-31-build-tools-are-tool-defs.md)

## Initial Discovery

[2026-10-31-agent-tools-build-tools-initial-discovery.md](./2026-10-31-agent-tools-build-tools-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-06-16-tddy-build.md](../todo/2026-06-16-tddy-build.md) | ⚠ During — **narrowed at wrap**. The *Hermetic sandboxing* bullet is closed here and removed from the entry. The other bullets (distributed cache, remote execution, watch mode, output publication, the cross-arch filter) stay |
| `tddy-build` | ℹ No `docs/code-issues/` directory: not analyzed. The executor change stays inside the existing functions' budgets |
| `tddy-build` description / `docs/architecture.md` ("no tddy-* deps") | ⚠ During. Already stale (it depends on `tddy-actions`). Corrected at wrap. The new `ActionSandbox` port keeps it free of sandbox crates |

## Affected Packages

- [ ] [`tddy-build`](../../../packages/tddy-build/README.md): the `tool` target is a `ToolDef` or a `ref`; target-output binaries; `tool_dep_ids` resolve through the tool registry; `BuildAction.sandbox`; the `ActionSandbox` port; cache key includes tool fingerprints
- [ ] [`tddy-actions`](../../../packages/tddy-actions/README.md): `ToolBinary` gains a `target` source (another target's output path), resolved by a caller-supplied resolver
- [ ] [`tddy-daemon-sandbox`](../../../packages/tddy-daemon-sandbox/README.md): `ActionSandbox` implementation over the action sandbox runtime, lowering inputs, outputs and tool closure
- [ ] [`tddy-bsp`](../../../packages/tddy-bsp/README.md): `build` / `build-list` wire the registry and `--hermetic`
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): the CLI flag passes through

## Responsibility

- `tddy_build::builtin::ToolTarget::{Inline(ToolDef), Ref { ref_: String }}`. The legacy `{bin_dir, commands}` shape is refused with a migration message naming the target.
- `tddy_actions::tool::ToolBinary.target: Option<TargetBinary { target: String, path: String }>` and `ToolHost::target_output(&TargetBinary) -> Option<PathBuf>`.
- `tddy_build::graph`: a tool target whose binary is a target output adds that target as a build-order predecessor of every action listing the tool.
- `tddy_build::executor`: `tool_dep_ids` resolve through `ToolRegistry::resolve_closure` and replace `tool_path`'s `bin_dir` join. PATH gets each resolved binary's link dir.
- `BuildAction.sandbox: bool` (proto field) and `BuildOptions.hermetic`.
- `tddy_build::ActionSandbox` — a trait `run(&ActionSpec, &[ResolvedTool], outputs) -> Result<ActionOutcome>`, injected into the executor.
- `tddy_daemon_sandbox::build_action_sandbox::DaemonActionSandbox` implements it: inputs are read-only mounts, outputs the only writable paths, tools are lowered via `lower_tools` and `lower_network`.
- The cache key includes the resolved tool binaries' fingerprints.
- `packages/tddy-build/examples/pipeline/app/BUILD.yaml` is migrated.

## Boundaries

- Non-hermetic actions run exactly as today.
- No `ActionType::COPY` or `TOOL`, no remote execution.
- Does not change `rg.yaml`, `tool_grants::lower_tools` or the egress policy semantics. It only calls them.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-defs (#790) | `ToolDef`, `ToolRegistry`, `ToolHost`, `resolve_closure`, `lower_tools` | BUILD tool targets are `ToolDef`s, and hermetic actions lower their tool closure | change the override rule or `lower_tools` |
| `n3` egress-allowlist | `NetworkSection`, `EgressPolicy`, `lower_network` | a hermetic action's network is `lower_network` of its tool closure | change `admits` or the union rule |
| `n2` claude-host | nothing consumed | sits above #791 only because of the line | touch `run` or `extensions` |

`ToolBinary.target` is a new field on #790's `ToolBinary`. It is additive, and resolution of the existing `command`/`env` sources is unchanged.

## Draft PR contract

The first push publishes:

- `ToolTarget` (its enum shape);
- `ToolBinary.target` and `ToolHost::target_output`;
- `BuildAction.sandbox` (proto plus serde helper);
- the `ActionSandbox` trait and the `DaemonActionSandbox` signature;
- the migrated example manifest;
- the failing tests below.

Unimplemented bodies are `// TODO(build-tools): implement`.

## Green wave

**Wave:** 2 of 7
**Greenable independently:** yes, once #790 is green, and #792 for the hermetic network test. Tests drive the executor with a real jail. Confirmed by the red phase: `BuildTools::load` needs #790's `ToolRegistry::{builtin, load}`, every acceptance test needs #790's `resolve_closure`, the hermetic tests need #790's `lower_tools`/`HostBinaryLibraries`, and only `a_hermetic_action_with_no_network_declaring_tool_has_no_network` needs #792's `lower_network`. No new edge
**Concurrent with:** #791 claude-host and #792 egress-allowlist
**Blocks:** nothing (no later node consumes the build integration)

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `builtin::ToolTarget { bin_dir, commands }`, of which only `bin_dir` is read. `executor::tool_path` joins `repo_root/bin_dir` onto PATH for actions listing the tool in `tool_dep_ids`.
- `ActionType::TOOL` and `COPY` are declared and unimplemented.
- Every action runs through `ProcessRuntime` with no sandbox. The docs list "hermetic sandboxing" as not implemented.
- The only `type: tool` user is `examples/pipeline/app/BUILD.yaml`.

## State B

- A tool target is a `ToolDef` or a `ref` into the registry. Its binary can be another target's output, which builds first.
- With `sandbox: true` or `--hermetic`, an action runs in a jail where its inputs are read-only, its outputs are the only writable paths, and only its tool closure is executable.
- Network is `lower_network` of the closure.
- The cache key covers the tools.

## Delta

### New
- `packages/tddy-daemon-sandbox/src/build_action_sandbox.rs`
- `packages/tddy-build/src/action_sandbox.rs`
### Modified
- `tddy-build` `builtin.rs`, `executor.rs`, `graph.rs`, `cache.rs`, `proto/tddy/build/v1/actions.proto`, `serde_helpers.rs`, the example manifest, the Cargo description.
- `tddy-actions` `tool/def.rs` and `resolve.rs`.
- `tddy-bsp` `build_cli.rs`.
- Docs at wrap: `docs/ft/build/tddy-build.md` § tool and § Hermetic actions; `packages/tddy-build/docs/architecture.md`.

## Testing Strategy

### Acceptance tests

**`tddy-build`: `packages/tddy-build/tests/build_tools_acceptance.rs`**

- [x] `an_action_using_a_tool_built_by_another_target_builds_that_target_first_and_finds_the_tool_on_path`
- [x] `a_ref_tool_target_resolves_from_the_registry_including_a_tddyhome_override`
- [x] `changing_a_tools_binary_invalidates_the_cache_of_every_action_using_it`
- [x] `the_legacy_bin_dir_tool_shape_is_refused_with_a_migration_message`

**`tddy-daemon-sandbox`: `packages/tddy-daemon-sandbox/tests/hermetic_build_action_acceptance.rs`**

- [x] `a_hermetic_action_writing_only_its_declared_output_succeeds`
- [x] `a_hermetic_action_writing_outside_its_outputs_fails_naming_the_path` (macOS Seatbelt here; Linux cgroups runs on CI)
- [x] `a_hermetic_action_execing_a_binary_outside_its_tool_closure_fails_naming_it`
- [x] `a_hermetic_action_with_no_network_declaring_tool_has_no_network`

### Unit / integration tests

- [x] `tddy-build` `builtin`: `ToolTarget` parsing of inline, ref and the legacy shape.
- [x] `tddy-build` `graph`: a target-output binary adds an ordering edge, and a cycle through a tool target is reported.
- [x] `tddy-build` `cache`: the key changes when a tool fingerprint changes and stays order-independent.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Manifest, graph, cache | Unit | Deterministic |
| Hermetic execution | Integration in a real jail | The claim is confinement; only the real backend shows it |

## Decisions & Trade-offs

- **`ActionSandbox` is a port.** `tddy-build` stays free of sandbox crates, and the daemon-sandbox implementation is wired by the binaries.
- **The legacy tool shape is refused, not migrated silently.** There is one in-repo user, and a silent reinterpretation would change which binaries an action may run.
- **Hermetic is opt-in** per action or per run. Defaulting it on is out of scope.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-build-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-10-31: pass 2 runs without per-node review
- [x] TDD Red — write failing unit/integration tests
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

**Published surface** (commit 2 of #793):

- `tddy_actions::tool::ToolBinary.target: Option<TargetBinary>` and `TargetBinary { target, path }` (`tool/def.rs`), additive with `#[serde(default)]`.
- `ToolHost::target_output(&TargetBinary) -> Option<PathBuf>` (`tool/resolve.rs`) is a **default method answering `None`**, so no existing `ToolHost` impl changed (the fixtures in `tddy-actions`, `tddy-daemon-sandbox` and `tddy-session-lifecycle` compile untouched). `resolve_closure` carries a `TODO(build-tools)` for the target branch beside #790's `TODO(tool-defs)`.
- `tddy_build::builtin::{ToolTarget::{Inline(Box<ToolDef>), Ref { ref_ }}, tool_target(target_id, fields)}`.
- `BuildAction.sandbox` — `bool sandbox = 10` in `actions.proto`, regenerated through the existing `build.rs`.
- `tddy_build::ActionSandbox` (`action_sandbox.rs`): `run(&ActionSpec, &[ResolvedTool]) -> BoxFuture<Result<ActionOutcome, BuildError>>`.
- `tddy_build::executor::{BuildTools { registry, host, sandbox }, BuildTools::load(tools_dir, host), BuildTools::with_sandbox, execute_target_with_tools, ExecuteOptions.hermetic}`.
- `tddy_build::cache::compute_cache_key_with_tools(action, inputs, tool_fingerprints)`.
- `tddy_daemon_sandbox::build_action_sandbox::DaemonActionSandbox::new(work_dir)` implementing `ActionSandbox`. `tddy-daemon-sandbox` gains a path dependency on `tddy-build` (no cycle: `tddy-build` depends only on `tddy-actions` and `tddy-task`).
- `examples/pipeline/app/BUILD.yaml` migrated: a new `tools:stamp-build` target installs `tools/bin/stamp` to `tools/out/stamp`, and `tools:bin` is an inline `ToolDef` whose binary is that output — the "built, then used" shape. No file was deleted.

**Red baseline** (`./dev cargo test -p tddy-build --no-fail-fast`, `-p tddy-daemon-sandbox --lib --test hermetic_build_action_acceptance`):

| Test | Fails because |
|---|---|
| `build_tools_acceptance::*` (4) | `todo!("build-tools: BuildTools::load")` |
| `hermetic_build_action_acceptance::*` (4) | `todo!("build-tools: BuildTools::load")`, then `DaemonActionSandbox::run` |
| `builtin::tests::{an_inline_tool_config_is_a_whole_tool_definition, a_ref_tool_config_names_a_registry_tool, the_legacy_tool_shape_is_refused_naming_the_target_and_the_migration, a_ref_with_inline_definition_fields_is_refused_naming_the_target}` | `todo!("build-tools: tool_target")` |
| `cache::tests::{key_changes_when_a_tool_binary_fingerprint_changes, key_with_tools_differs_from_the_key_without_them, key_is_independent_of_tool_fingerprint_order}` | `todo!("build-tools: compute_cache_key_with_tools")` |
| `graph::tests::a_tool_built_by_another_target_orders_that_target_before_every_user_of_the_tool` | assertion: `build_order("app")` is `["app"]`, expected `["stamp:bin", "tools:stamp", "app"]` (`target_edges` has the `TODO(build-tools)`) |
| `graph::tests::a_cycle_through_a_tool_target_is_reported_naming_it` | assertion: `from_manifests` accepts the cycle |
| `example_pipeline::pipeline_builds_successfully_through_a_tool_target` | **was green; intentionally red.** The migrated manifest reaches the legacy `tool_path`, which refuses it (`invalid tool config: unknown field schema_version`). Green's executor resolution makes it pass |

Every other test in `tddy-build` (34 lib + 14 + 4 + 1 + 4 + 5) and the `tddy-daemon-sandbox` lib (7) passes.

**Pre-existing failures** (not this node's): 33 `tddy-actions` tests panic on parent TODOs — `tool-defs: ToolRegistry::builtin` (17), `claude-host: render_argv` (9), `egress-allowlist: HostPattern::parse` (6), `egress-allowlist: Egress::deserialize` (1). `tddy-daemon-sandbox/tests/sandbox_stdio_seatbelt_acceptance.rs` does not compile on macOS (`SandboxHandle` not in scope) and was excluded from every scoped command.

**Deviations from the planned surface:**

- `ToolTarget::Inline` boxes its `ToolDef` (`clippy::large_enum_variant`: 312 bytes against a `String`).
- `ActionSandbox::run` takes no separate `outputs`: the writable paths are `ActionSpec::outputs`, already absolute. It returns a `BoxFuture` (`futures`, already a dependency) so `tddy-build` gains no `async-trait`.
- "`BuildOptions.hermetic`" is `ExecuteOptions.hermetic` — `BuildOptions` is `tddy-toolcall`'s relay wire type, which this node does not touch.
- The contract adds `BuildTools`, `execute_target_with_tools` and `compute_cache_key_with_tools` instead of changing `execute_target`'s and `compute_cache_key`'s signatures, which ~60 call sites in seven crates use. Green makes `execute_target` delegate and folds the two cache functions together.
- No serde helper for `sandbox`: a proto `bool` deserializes through the message-level `#[serde(default)]`.
- The PRD's example wrote `binary: { target, path }`; `ToolBinary.command` is #790's required field, so the shape is `binary: { command, target: { target, path } }`. The PRD example is corrected.
- An inline tool's `id` is its tool id; `tool_dep_ids` name *targets*, mapped to the inline `id` or to `ref`. An inline id the registry already holds is refused as a duplicate (documented on `ToolTarget`).
- `tddy-bsp` / `tddy-tools` `--hermetic` wiring is not in the draft contract: no test drives it. Green adds the flag and builds `BuildTools` from `<tddyhome>/tools` with `DaemonActionSandbox`.
- #790's `tool_registry_acceptance.rs` gained `target: None` in its two `ToolBinary` literals — the only edit outside this node's files, forced by the additive field.

**`TODO(build-tools)` deletions needing developer approval at green:** `builtin::{LegacyToolTarget, tool_bin_dir}`, `executor::tool_path`, and the two tests that pin the legacy shape — `builtin::tests::tool_extracts_bin_dir` and `build_acceptance::tool_bin_dir_is_prepended_to_action_path` (the latter rewritten or deleted in favour of `the_legacy_bin_dir_tool_shape_is_refused_with_a_migration_message`). `build_acceptance::MIXED_YAML` still lists a legacy tool target; `build-list` never parses it, so it stays valid until green decides.
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
