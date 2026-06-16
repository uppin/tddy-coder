# tddy-build architecture

Standalone, Bazel-inspired, content-addressed build **engine** plus a wiring point for build plugins. No `tddy-*` dependencies and **no knowledge of any specific ecosystem target type** — `tddy-tools` and `tddy-coder` depend on it; `tddy-core` only exposes an extension point. The recipe crates (`tddy-build-rust`, `tddy-build-typescript`, `tddy-build-docker`) depend on it and implement its plugin trait.

## Manifests → open serde schema

`BUILD.yaml` files deserialize into plain **serde structs** in `src/manifest.rs` — `BuildManifest` / `BuildTarget` / `TargetConfig`. A target's `config` is open: a `type` tag plus a `#[serde(flatten)] fields: serde_yaml::Value` payload the handler (not the engine) interprets.

- `manifest::load_build_manifest(yaml) -> BuildManifest` is the entry point; `BuildManifest`/`BuildTarget` carry `default` + `deny_unknown_fields`.
- `TargetConfig` = `{ r#type: String, #[serde(flatten)] fields }`; the `type` key is extracted into `r#type`, so `fields` holds only the handler's keys.
- Only `BuildAction` (the engine↔plugin contract + cache-key input) and the cache types stay proto, compiled by `build.rs`.
- `serde_helpers`: string↔`i32` converters for `ActionType` (`command`/`copy`/`tool`) and `OutputKind` (`file`/`directory`, `dir` alias), wired onto the proto `BuildAction`/`OutputDecl` fields.

### Proto schema (`proto/tddy/build/v1/`)
- `actions.proto`: `BuildAction`, `FileSet`, `OutputDecl`, `ActionType`, `OutputKind`.
- `cache.proto`: `ActionCacheEntry`, `FileFingerprint`.

(The former `manifest.proto`/`targets.proto` were removed — the manifest is serde, and there is no closed `oneof` of target types.)

## Plugins and built-ins

- **`plugin.rs`** — the `BuildPlugin` trait (`type_names()` + `lower(&LowerContext)`), `LowerContext` (type tag, target id/name, deps, config fields), and `PluginRegistry` (maps `config.type` → plugin; last registration wins). The binaries assemble the registry and pass `&PluginRegistry` into the engine.
- **`builtin.rs`** — the three structural types the engine keeps built in: `script` (generic command), `tool` (provides `bin_dir` on dependents' `PATH`), `group` (member ids become build-order predecessors). Their config is parsed on demand from the open `TargetConfig.fields`.

## Pipeline

1. **discovery** — glob `**/{BUILD,build}.{yaml,yml}` under the repo root; parse each into a `BuildManifest`.
2. **lower** — `lower_target(target, &registry)` dispatches by `config.type`: `script`→declared command (built-in); `tool`/`group`→no own action (built-in); any other type→the registered `BuildPlugin` (`unknown target type: <name>` if none). Explicit `actions` are kept and run before the lowered one.
3. **graph** — `BuildGraph::from_manifests` flattens targets (rejecting duplicate ids), detects target-level cycles (deps + group members, read from the open config), and exposes `build_order` (deps-first) and `waves(&registry)` (Kahn topological levels). Action-level edges are inferred from input-glob/output-path overlap.
4. **cache** — `compute_cache_key` = `sha256:` over action id/type/command/working-dir/env(sorted)/input fingerprints(sorted)/outputs(sorted)/tool deps(sorted); order-independent. `lookup_cache` is a hit only when the recorded key matches and every declared output still exists. `persist_cache` writes atomically (tmp + `sync_all` + rename). `CacheMode`: `ReadWrite` (default) / `ReadOnly` / `Offline`.
5. **executor** — `execute_target` builds dependencies/group members first, runs each target's actions wave-by-wave in parallel (`futures::join_all`), checks/populates the cache, supports `--dry-run` (emit argv only), and prepends each `ToolTarget`'s `bin_dir` onto the action's `PATH`.

## Entry points (`service`)

`build_list_json` and `build_json` return the JSON shapes shared by the local CLI and the relay executor, so both paths produce identical output.

## Consumers

- **tddy-tools** `build` / `build-list` subcommands: run `tddy-build` locally, or relay over `TDDY_SOCKET`.
- **tddy-core** `toolcall::BuildExecutor` trait + process-global registry + listener handlers for `build` / `build-list` — **no dependency on tddy-build** (returns "build support not enabled" when nothing is registered).
- **tddy-coder** implements `BuildExecutor` on top of `tddy-build` and registers it before starting the toolcall listener, so relayed build requests run co-located with the worktree.

## Not yet implemented (v1)

Distributed/parent-fallback cache, hermetic sandboxing, full remote build execution, watch mode, a final output-publication convention (`.tddy-build/out/{target_id}/` staging only), and cross-compilation architecture filtering. See `docs/dev/TODO.md`.
