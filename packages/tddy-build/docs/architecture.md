# tddy-build architecture

Standalone, Bazel-inspired, content-addressed build **engine** plus a wiring point for build plugins. Its only `tddy-*` dependencies are `tddy-actions` (tool definitions, `ActionSpec`) and `tddy-task` (the process runtime); it has **no sandbox dependency** — hermetic actions run through the `ActionSandbox` port, implemented in `tddy-daemon-sandbox` — and **no knowledge of any specific ecosystem target type** — `tddy-tools` and `tddy-coder` depend on it; `tddy-core` only exposes an extension point. The recipe crates (`tddy-build-rust`, `tddy-build-typescript`, `tddy-build-docker`, `tddy-build-buildroot`, `tddy-build-qemu`) depend on it and implement its plugin trait.

## Manifests → open serde schema

`BUILD.yaml` files deserialize into plain **serde structs** in `src/manifest.rs` — `BuildManifest` / `BuildTarget` / `TargetConfig`. A target's `config` is open: a `type` tag plus a `#[serde(flatten)] fields: serde_yaml::Value` payload the handler (not the engine) interprets.

- `manifest::load_build_manifest(yaml) -> BuildManifest` is the entry point; `BuildManifest`/`BuildTarget` carry `default` + `deny_unknown_fields`.
- `TargetConfig` = `{ r#type: String, #[serde(flatten)] fields }`; the `type` key is extracted into `r#type`, so `fields` holds only the handler's keys.
- Only `BuildAction` (the engine↔plugin contract + cache-key input) and the cache types stay proto, compiled by `build.rs`.
- `serde_helpers`: string↔`i32` converters for `ActionType` (`command`/`copy`/`tool`) and `OutputKind` (`file`/`directory`, `dir` alias), wired onto the proto `BuildAction`/`OutputDecl` fields.

### Proto schema (`proto/tddy/build/v1/`)
- `actions.proto`: `BuildAction` (including `bool sandbox` — run this action hermetically), `FileSet`, `OutputDecl`, `ActionType`, `OutputKind`.
- `cache.proto`: `ActionCacheEntry`, `FileFingerprint`.

(The former `manifest.proto`/`targets.proto` were removed — the manifest is serde, and there is no closed `oneof` of target types.)

## Plugins and built-ins

- **`plugin.rs`** — the `BuildPlugin` trait (`type_names()` + `lower(&LowerContext)`), `LowerContext` (type tag, target id/name, deps, config fields), and `PluginRegistry` (maps `config.type` → plugin; last registration wins). The binaries assemble the registry and pass `&PluginRegistry` into the engine.
- **`builtin.rs`** — the three structural types the engine keeps built in: `script` (generic command), `tool` (`ToolTarget::{Inline(Box<ToolDef>), Ref { ref_ }}`, parsed by `tool_target(target_id, fields)`; the legacy `bin_dir`/`commands` keys are refused naming the target), `group` (member ids become build-order predecessors). Their config is parsed on demand from the open `TargetConfig.fields`.

## Plugin IO helpers

`src/io.rs` provides three helpers plugins use to populate `BuildAction.inputs` / `.outputs` from their open config:

- `OutputSpec` — `{ path: String, kind: String }` serde struct; the `kind` field maps `"file"` → `OutputKind::File`, `"dir"` → `OutputKind::Directory`.
- `srcs_to_inputs(srcs: &[String], root: &str) -> Vec<FileSet>` — wraps a list of glob patterns into a `FileSet` (optionally rooted at `root`).
- `outputs_to_decls(outputs: &[OutputSpec]) -> Result<Vec<OutputDecl>, BuildError>` — maps `OutputSpec` → `OutputDecl`, rejecting unknown `kind` values.

Plugins that populate these fields participate in the content-addressed cache: inputs determine the cache key (fingerprinted by mtime+size), and outputs are checked for existence on cache lookup.

## Pipeline

The engine instruments each seam with `log::` calls (discovery, lowering, cycle detection, cache hit/miss, execution). Set `RUST_LOG=tddy_build=debug` (or `trace`) to observe the full build graph.

1. **discovery** — glob `**/{BUILD,build}.{yaml,yml}` under the repo root; parse each into a `BuildManifest`. Discovered manifest count logged at `debug`.
2. **lower** — `lower_target(target, &registry)` dispatches by `config.type`: `script`→declared command (built-in); `tool`/`group`→no own action (built-in); any other type→the registered `BuildPlugin` (`unknown target type: <name>` if none). Explicit `actions` are kept and run before the lowered one.
3. **graph** — `BuildGraph::from_manifests` flattens targets (rejecting duplicate ids), detects target-level cycles (deps + group members, read from the open config), and exposes `build_order` (deps-first) and `waves(&registry)` (Kahn topological levels). Cycle detection emits a `warn!` naming the offending target. A tool target whose `binary.target` names another target adds that target as a build-order predecessor of every target with an action listing the tool in `tool_dep_ids` (declared actions only — plugin-lowered ones are not read at graph time, see [docs/dev/todo/2026-11-02-plugin-lowered-tool-deps-get-no-build-order.md](../../../docs/dev/todo/2026-11-02-plugin-lowered-tool-deps-get-no-build-order.md)); a cycle through it is reported. Action-level edges are inferred from input-glob/output-path overlap — an edge is added when Action A declares an `output.path` that matches a glob in Action B's `inputs`; plugins must explicitly declare intermediates to enforce ordering.
4. **cache** — `compute_cache_key` = `sha256:` over action id/type/command/working-dir/env(sorted)/input fingerprints(sorted)/outputs(sorted)/tool deps(sorted); order-independent. `compute_cache_key_with_tools` folds in the resolved tool binaries' fingerprints (sorted; unchanged key when there are none), and `compute_run_cache_key` — what the executor uses — additionally records whether the action ran hermetically. `lookup_cache` is a hit only when the recorded key matches and every declared output still exists. `persist_cache` writes atomically (tmp + `sync_all` + rename). `CacheMode`: `ReadWrite` (default) / `ReadOnly` / `Offline`. Plugins declare cacheable inputs/outputs via the `io` helper (`OutputSpec`, `srcs_to_inputs`, `outputs_to_decls`) in their open config; the executor glob-expands `srcs` patterns for fingerprinting.
5. **executor** — `execute_target` (delegating to `execute_target_with_tools` with `BuildTools::default()`) builds dependencies/group members first, runs each target's actions wave-by-wave in parallel (`futures::join_all`), checks/populates the cache, supports `--dry-run` (emit argv only), and **creates declared output parent directories** before running the action (many tools — `docker build --iidfile`, `qemu-img convert`, most compilers — do not create intermediate directories themselves).

## Tools and hermetic actions

- **`BuildTools { registry, host, sandbox }`** — what a run resolves tools against. `BuildTools::load(tools_dir, host)` is the built-in `ToolRegistry` plus `tools_dir` overrides; `with_sandbox(Arc<dyn ActionSandbox>)` attaches the jail. Passed via `execute_target_with_tools` / `service::build_json_with_tools`; `ExecuteOptions.hermetic` jails every action.
- **`executor::tool_run`** — `ToolRun::new` reads the tool targets the run's actions name: an inline definition joins a clone of the registry (keyed to its target id), a `ref` must name a registry tool. Per action it resolves the closure with `ToolRegistry::resolve_closure` against a `TargetHost`, which answers `ToolHost::target_output` with the producing target's output under the repo root (already built by the graph order) and defers everything else to the tools' host. The resolved binaries' link directories go on `PATH`, and their fingerprints into the cache key.
- **`action_sandbox::ActionSandbox`** — the port: `run(&ActionSpec, &[ResolvedTool]) -> BoxFuture<Result<ActionOutcome, BuildError>>`. The spec's inputs are concrete read-only paths (globs expanded by `tool_run`), its `outputs` the writable paths. A jail refusal is an `Ok` outcome with a non-zero exit and the refusal in `stderr`; `Err` means the jail could not start. A hermetic action with no sandbox is refused, never run unconfined. The implementation is `tddy_daemon_sandbox::build_action_sandbox::DaemonActionSandbox`, wired by `tddy-bsp`'s `build` (`--hermetic`), so this crate has no sandbox dependency.

## Entry points (`service`)

`build_list_json` and `build_json` return the JSON shapes shared by the local CLI and the relay executor, so both paths produce identical output.

## Recipe plugin crates

| Crate | Type name(s) | What it lowers to |
|---|---|---|
| `tddy-build-rust` | `rust_binary`, `rust_library` | `cargo build -p <pkg>` |
| `tddy-build-typescript` | `typescript_build` | `bun run build` |
| `tddy-build-docker` | `docker_image` | `docker build --iidfile` |
| `tddy-build-buildroot` | `buildroot_image` | `make O=<out> <defconfig>` then `make O=<out>` in `buildroot_dir` |
| `tddy-build-qemu` | `qemu_disk_image` | `qemu-img convert -f raw -O qcow2 <input> <output>` |

Each crate ships an example `BUILD.yaml` under `examples/` and integration tests in `tests/` that exercise the plugin against real tooling.

## Consumers

- **tddy-tools** `build` / `build-list` subcommands: run `tddy-build` locally, or relay over `TDDY_SOCKET`. Assembles the plugin registry from `tddy-build-rust`, `tddy-build-typescript`, `tddy-build-docker`.
- **tddy-core** `toolcall::BuildExecutor` trait + process-global registry + listener handlers for `build` / `build-list` — **no dependency on tddy-build** (returns "build support not enabled" when nothing is registered).
- **tddy-coder** implements `BuildExecutor` on top of `tddy-build` and registers it before starting the toolcall listener, so relayed build requests run co-located with the worktree. Assembles the same plugin registry as `tddy-tools`.
- **Workspace `BUILD.yaml` configs** — every package in the workspace ships a `packages/<pkg>/BUILD.yaml` (`rust_library`/`rust_binary` targets with deps chains and `srcs` globs); root `BUILD.yaml` provides an `all:build` group over the main deliverables (`tddy-coder`, `tddy-tools`, `tddy-daemon`, `tddy-web`). Run `tddy-tools build-list --repo-dir .` to enumerate all targets.

## Not yet implemented (v1)

Distributed/parent-fallback cache, full remote build execution, watch mode, a final output-publication convention (`.tddy-build/out/{target_id}/` staging only), and cross-compilation architecture filtering. See [docs/dev/todo/2026-06-16-tddy-build.md](../../../docs/dev/todo/2026-06-16-tddy-build.md). Hermetic actions' limits are in [tddy-build § Hermetic actions](../../../docs/ft/build/tddy-build.md#hermetic-actions).
