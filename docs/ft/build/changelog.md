# Build — changelog

## 2026-06-16 — tddy-build plugin architecture

- `tddy-build` is now a generic engine plus a wiring point for build plugins — it has no knowledge of any specific ecosystem target type.
- A target's `config` is open (`type` tag + arbitrary fields); the engine dispatches `type` to a registered `BuildPlugin` or to a built-in structural type. The `BUILD.yaml` authoring format is unchanged.
- `script` / `tool` / `group` stay built into the engine (the build-graph vocabulary); `rust_binary` / `rust_library` / `typescript` / `docker_image` moved to plugin crates `tddy-build-rust`, `tddy-build-typescript`, `tddy-build-docker`.
- Unknown, unregistered target types fail fast with `unknown target type: <name>`.

## 2026-06-16 — tddy-build content-addressed build system

- New `tddy-build` system: `BUILD.yaml` targets (rust_binary, rust_library, typescript, docker_image, script, tool, group) deserialized directly into proto types.
- Global build DAG with topological waves, cycle detection, and a SHA-256 content-addressed action cache (atomic writes; `readwrite`/`readonly`/`offline` modes).
- `tddy-tools build` / `build-list` subcommands (local execution + `TDDY_SOCKET` relay); the agent discovers targets via `build-list`, mirroring session actions.
- Relay served via a `tddy_core::BuildExecutor` extension point registered by `tddy-coder`; `tddy-core` stays decoupled from `tddy-build`.
