# Development TODO

## Future Enhancements

### tddy-sandbox-cgroups (source: finish-stdio-ipc-migration changeset, 2026-07-02)

- **Verify `--stdio` jail-spawn piping through a real Linux jail** — `spawn_plan` now pipes
  stdin/stdout (instead of leaving stdout on its prior default) when `--stdio` is in the command,
  mirroring `tddy-sandbox-darwin::spawn_plan`. Compile-checked only (the crate is
  `#[cfg(target_os = "linux")]`-gated and the dev environment that made this change has no Linux
  box); needs a real-jail run in Linux CI to confirm the daemon's now-stdio-only session control
  channel (`docs/dev/1-WIP/finish-stdio-ipc-migration.md`) actually works cross-platform.

### tddy-sandbox-cgroups (source: sandbox-builder changeset, 2026-06-28)

- **Minimal RO-root `pivot_root`** — the sandbox-builder changeset lands read-only bind-mounts of each declared `ReadSpec` inside the rootless jail, but the jail still shares the host filesystem root. Build a minimal tmpfs root, bind only the plan's reads + writable project/scratch/egress, then `pivot_root` into it for full filesystem write-confinement.

### tddy-build (source: tddy-build-bazel-system changeset, 2026-06-16)

- **Distributed cache / parent-fallback** — remote shared cache layer (maker-build pattern). Deferred to v2.
- **Hermetic sandboxing** — isolate action execution; v1 uses PATH + cwd discipline only.
- **Full remote build execution** — `TDDY_SOCKET` relay covers co-located sessions; true remote/distributed build deferred.
- **Watch mode** — incremental rebuild on file change.
- **Output-publication convention** — finalize the published-artifact layout (maker-build publishes to `dist/{name}/`); v1 stages under `.tddy-build/out/{target_id}/` only.
- **Cross-compilation architecture filter** — port `ensure_action_architecture()` from `session_actions` for ToolTargets that ship per-arch binaries.
