# tddy-build: content-addressed build system

**Product area:** Build
**Updated:** 2026-06-16
**Status:** In development (TDD red phase)

## Summary

`tddy-build` is a standalone, Bazel-inspired build system for repository artifacts. Build targets are declared in `BUILD.yaml` files and deserialize **directly into prost-generated proto types** (no parallel serde struct layer). Targets lower to a global DAG of build actions, executed wave-by-wave with a content-addressed (SHA-256) action cache. The system is driven by `tddy-tools build` / `tddy-tools build-list` and is discoverable by the coding agent the same way session actions are — via the CLI.

It complements, but is distinct from, the two existing "action" concepts: `tddy-core` **session actions** (ephemeral agent capabilities) and the workflow **action cache** (per-session LLM fingerprint cache). Neither is a build graph.

## Target types

A `BUILD.yaml` `BuildTarget` selects exactly one typed config (a proto `oneof`), and/or declares explicit `actions`:

| Type | Lowers to |
|------|-----------|
| `rust_binary` | `cargo build -p <pkg> --bin <name> [--features …] [--release] [--target <triple>]` |
| `rust_library` | `cargo build -p <pkg> [--features …] [--release]` |
| `typescript` | `bun run <build_script>` in `package_dir` |
| `docker_image` | `docker build -f <dockerfile> -t <tag> [--build-arg …] <context>` |
| `script` | the declared `command` argv |
| `tool` | no build action — registers its `bin_dir` on the `PATH` of dependents |
| `group` | expands to its member targets' actions |

All seven types execute. Hermetic tests assert command construction and `--dry-run` output for every type; real subprocess execution is covered for `script`/`tool`, with `cargo`/`bun`/`docker` execution behind environment availability gates.

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

YAML keys map 1:1 onto proto fields; the typed config is internally tagged by `type`. Enum-valued fields (`ActionType`, `OutputKind`) are authored as snake_case strings. Unknown fields are rejected.

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
tddy-build  (standalone — no tddy-* deps)
   ▲ used by tddy-tools (local exec / relay client) and tddy-coder (executor impl)
tddy-core   (BuildExecutor trait + wire types + ToolCallResponse variants; no tddy-build dep)
```

## Out of scope (v1)

Distributed/parent-fallback cache; hermetic sandboxing; full remote build execution; watch mode; final output-publication convention (`.tddy-build/out/{target_id}/` staging only); cross-compilation architecture filtering. See `docs/dev/TODO.md`.

## Related

- `docs/dev/1-WIP/tddy-build-bazel-system.md` — changeset
- `packages/tddy-build/` — implementation
- Prior art: `~/Code/makers-lt/maker-build` (two-phase TypeScript build system)
