# tddy-build architecture

Standalone, Bazel-inspired, content-addressed build system for repository artifacts. No `tddy-*` dependencies — `tddy-tools` and `tddy-coder` depend on it; `tddy-core` only exposes an extension point.

## Manifests → proto types

`BUILD.yaml` files deserialize **directly into prost-generated proto types** (`src/proto`, compiled by `build.rs`). There is no parallel serde struct layer:

- `build.rs` attaches `serde::{Serialize, Deserialize}` to every message via `type_attribute(".")`, adds per-message `default` + `deny_unknown_fields`, marks the `BuildTarget.config` oneof internally tagged (`#[serde(tag = "type")]`), and routes enum fields through `serde_helpers`.
- `serde_helpers`: string↔`i32` converters for `ActionType` (`command`/`copy`/`tool`) and `OutputKind` (`file`/`directory`, `dir` alias).
- `manifest::load_build_manifest(yaml) -> BuildManifest` is the entry point.

### Proto schema (`proto/tddy/build/v1/`)
- `manifest.proto`: `BuildManifest`, `BuildTarget` (`oneof config` over the 7 target types, plus explicit `actions`).
- `targets.proto`: `RustBinaryTarget`, `RustLibraryTarget`, `TypeScriptTarget`, `DockerImageTarget`, `ScriptTarget`, `ToolTarget`, `TargetGroupTarget`.
- `actions.proto`: `BuildAction`, `FileSet`, `OutputDecl`, `ActionType`, `OutputKind`.
- `cache.proto`: `ActionCacheEntry`, `FileFingerprint`.

## Pipeline

1. **discovery** — glob `**/{BUILD,build}.{yaml,yml}` under the repo root; parse each into a `BuildManifest`.
2. **lower** — `lower_target` turns a target into concrete `BuildAction`s: `rust_binary`/`rust_library`→`cargo build`, `typescript`→`bun run` (cwd = `package_dir`), `docker_image`→`docker build`, `script`→declared command, `tool`/`group`→no own action. Explicit `actions` are kept and run before the lowered one.
3. **graph** — `BuildGraph::from_manifests` flattens targets (rejecting duplicate ids), detects target-level cycles (deps + group members), and exposes `build_order` (deps-first) and `waves` (Kahn topological levels). Action-level edges are inferred from input-glob/output-path overlap.
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
