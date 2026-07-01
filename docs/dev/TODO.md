# Development TODO

## Future Enhancements

### tddy-core (source: stdio-transport-for-grpc-binaries changeset, 2026-07-01)

- **Migrate the toolcall listener to tddy-rpc/tddy-stdio** — `tddy-core/src/toolcall/listener.rs` is a third bespoke newline-delimited-JSON protocol (`submit`/`ask`/`approve`/`list-actions`/`build`) between `tddy-coder` and the Claude Code CLI subprocess it spawns, distinct from the sandbox tool-IPC and gRPC-over-UDS relay this changeset migrates. Same category of problem, same fix would apply, deferred to keep this changeset scoped.

### tddy-sandbox-cgroups (source: sandbox-builder changeset, 2026-06-28)

- **Minimal RO-root `pivot_root`** — the sandbox-builder changeset lands read-only bind-mounts of each declared `ReadSpec` inside the rootless jail, but the jail still shares the host filesystem root. Build a minimal tmpfs root, bind only the plan's reads + writable project/scratch/egress, then `pivot_root` into it for full filesystem write-confinement.

### tddy-build (source: tddy-build-bazel-system changeset, 2026-06-16)

- **Distributed cache / parent-fallback** — remote shared cache layer (maker-build pattern). Deferred to v2.
- **Hermetic sandboxing** — isolate action execution; v1 uses PATH + cwd discipline only.
- **Full remote build execution** — `TDDY_SOCKET` relay covers co-located sessions; true remote/distributed build deferred.
- **Watch mode** — incremental rebuild on file change.
- **Output-publication convention** — finalize the published-artifact layout (maker-build publishes to `dist/{name}/`); v1 stages under `.tddy-build/out/{target_id}/` only.
- **Cross-compilation architecture filter** — port `ensure_action_architecture()` from `session_actions` for ToolTargets that ship per-arch binaries.
