# tddy-build: content-addressed build system

**Product area:** Build
**Updated:** 2026-06-16
**Status:** Implemented

## Summary

`tddy-build` is a standalone, Bazel-inspired build system for repository artifacts. It is a **generic build engine plus a wiring point for build plugins** — it has no knowledge of any language/ecosystem build target. Build targets are declared in `BUILD.yaml` files; the engine resolves each target's `type` to a registered `BuildPlugin` (or to one of its built-in structural types) which lowers the target into build actions. Targets lower to a global DAG of build actions, executed wave-by-wave with a content-addressed (SHA-256) action cache. The system is driven by `tddy-tools build` / `tddy-tools build-list` and is discoverable by the coding agent the same way session actions are — via the CLI.

It complements, but is distinct from, the two existing "action" concepts: `tddy-core` **session actions** (ephemeral agent capabilities) and the workflow **action cache** (per-session LLM fingerprint cache). Neither is a build graph.

## Plugin architecture

The engine knows nothing about specific build targets. A `BuildPlugin` declares the
`type` tag(s) it handles and lowers a target's config into `BuildAction`s:

```rust
pub trait BuildPlugin: Send + Sync {
    fn type_names(&self) -> &'static [&'static str]; // e.g. ["rust_binary", "rust_library"]
    fn lower(&self, ctx: &LowerContext) -> Result<Vec<BuildAction>, BuildError>;
}
```

Plugins are collected into a `PluginRegistry`, which the binaries assemble and pass
into the engine API (`service::build_json` / `build_list_json`). The
language/ecosystem recipes live in their **own crates**:

| Crate | Type(s) | Lowers to |
|-------|---------|-----------|
| `tddy-build-rust` | `rust_binary` | `cargo build -p <pkg> --bin <name> [--features …] [--release] [--target <triple>]` |
| `tddy-build-rust` | `rust_library` | `cargo build -p <pkg> [--features …] [--release]` |
| `tddy-build-typescript` | `typescript` | `bun run <build_script>` in `package_dir` |
| `tddy-build-docker` | `docker_image` | `docker build -f <dockerfile> -t <tag> [--build-arg …] <context>` |

### Built-in structural types

The engine keeps three types built in, because they are part of the build-graph
vocabulary itself rather than ecosystem recipes:

| Type | Behavior |
|------|----------|
| `script` | the declared `command` argv (generic escape hatch) |
| `tool` | no build action — registers its `bin_dir` on the `PATH` of dependents |
| `group` | no own action — its members become build-order predecessors |

A target whose `type` is neither a built-in nor a registered plugin fails with
`unknown target type: <name>`. Hermetic tests assert command construction and
`--dry-run` output; real subprocess execution is covered for `script`/`tool`, with
`cargo`/`bun`/`docker` execution behind environment availability gates.

## Authoring format

```yaml
schema_version: 1
targets:
  - id: "packages/app:bin"
    name: "App binary"
    deps: ["packages/core:lib"]
    config:
      type: rust_binary
      package: app
      bin_name: app
      profile: release
```

The `config` map is open: `type` selects the plugin/built-in, and the remaining keys
are that handler's config (parsed by the plugin). Explicit `actions` and enum-valued
action fields (`ActionType`, `OutputKind`, authored as snake_case strings) are
unchanged. `BuildAction` and the cache types remain proto messages — the stable
engine↔plugin contract.

## Action cache

- Location: `{repo_root}/.tddy-build/cache/{target_id}/{action_id}.json`.
- Key: `sha256:<hex>` over the action id, type, command, env, input file fingerprints (`path:size:mtime_ms`), declared outputs, and tool deps — order-independent.
- Hit requires: stored key matches the recomputed key **and** every declared output still exists on disk.
- Writes are atomic (tmp + `sync_all` + rename), mirroring `tddy-core`'s action-cache flush.
- Modes: `readwrite` (default), `readonly`/`offline` (read local, never write); `--no-cache` bypasses both.

## CLI & relay

- `tddy-tools build-list --repo-dir <dir> [--query …] [--limit …] [--offset …]` → `{"targets":[…],"total":N}`.
- `tddy-tools build --repo-dir <dir> --target <id> [--no-cache] [--dry-run]` → build record JSON.
- Both are relay-capable via `TDDY_SOCKET`: when set, the request is forwarded to the host session, where `tddy-coder` has registered a `tddy_core::BuildExecutor`. `tddy-core` defines the extension trait and wire types only — it has **no** dependency on `tddy-build`; the dependency and wiring live in `tddy-coder`.

## Crate boundaries

```
tddy-build  (standalone engine + plugin wiring point — no tddy-* deps)
   ▲ implemented-against by the plugin crates:
   │     tddy-build-rust, tddy-build-typescript, tddy-build-docker
   ▲ assembled into a PluginRegistry by:
   │     tddy-tools  (local exec / relay client)
   │     tddy-coder  (executor impl; relay host)
tddy-core   (BuildExecutor trait + wire types + ToolCallResponse variants; no tddy-build dep)
```

`tddy-build` depends on none of the plugin crates; the plugin crates depend only on
`tddy-build`. The binaries (`tddy-tools`, `tddy-coder`) are the only places that know
the concrete plugin set — they register `RustPlugin`/`TypeScriptPlugin`/`DockerPlugin`
into a `PluginRegistry` and pass it into the engine.

## Out of scope (v1)

Distributed/parent-fallback cache; hermetic sandboxing; full remote build execution; watch mode; final output-publication convention (`.tddy-build/out/{target_id}/` staging only); cross-compilation architecture filtering. See `docs/dev/TODO.md`.

## Related

- `packages/tddy-build/docs/architecture.md` — crate architecture; `docs/dev/changesets.md` — cross-package changeset history
- `packages/tddy-build/` — implementation
- Prior art: `~/Code/makers-lt/maker-build` (two-phase TypeScript build system)
