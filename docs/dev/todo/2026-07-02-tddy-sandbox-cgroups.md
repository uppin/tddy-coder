# 2026-07-02 — tddy-sandbox-cgroups

**Category:** Future enhancement
**Source:** finish-stdio-ipc-migration changeset, 2026-07-02

- **Verify `--stdio` jail-spawn piping through a real Linux jail** — `spawn_plan` now pipes
  stdin/stdout (instead of leaving stdout on its prior default) when `--stdio` is in the command,
  mirroring `tddy-sandbox-darwin::spawn_plan`. Compile-checked only (the crate is
  `#[cfg(target_os = "linux")]`-gated and the dev environment that made this change has no Linux
  box); needs a real-jail run in Linux CI to confirm the daemon's now-stdio-only session control
  channel (`docs/dev/1-WIP/finish-stdio-ipc-migration.md`) actually works cross-platform.
