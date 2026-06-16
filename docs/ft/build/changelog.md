# Build — changelog

## 2026-06-16 — tddy-build content-addressed build system

- New `tddy-build` system: `BUILD.yaml` targets (rust_binary, rust_library, typescript, docker_image, script, tool, group) deserialized directly into proto types.
- Global build DAG with topological waves, cycle detection, and a SHA-256 content-addressed action cache (atomic writes; `readwrite`/`readonly`/`offline` modes).
- `tddy-tools build` / `build-list` subcommands (local execution + `TDDY_SOCKET` relay); the agent discovers targets via `build-list`, mirroring session actions.
- Relay served via a `tddy_core::BuildExecutor` extension point registered by `tddy-coder`; `tddy-core` stays decoupled from `tddy-build`.
