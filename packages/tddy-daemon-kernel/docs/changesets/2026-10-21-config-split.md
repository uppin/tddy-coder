# 2026-10-21 — `config.rs` split into per-concern modules

**Type:** Refactor

`config.rs` is cut into `config.rs` plus eight modules under `src/config/` — `agent_cli`, `daemon_methods`,
`sections`, `env_overrides`, `timings`, `screen_sharing`, `sandbox_paths`, `access` — by `tddy-tools
restructure`, in eight plans (01, 03–09), one commit each. No behaviour change, no new dependency;
`restructure verify --against origin/master` reports every statement accounted for (7 re-pointed through a
module qualifier, 12 visibility-normalised). `ListenConfig` and `LiveKitConfig` stay in `config.rs`: the serde
defaults they name are not contiguous with them.

**Measurement.** Production lines of `config.rs`, counted as production code excluding `#[cfg(test)]` items:
**1,575 → 438** (398 non-blank). The engine left `#[cfg(test)] use …;` lines at the top of the file, so a count
that stops at the first `#[cfg(test)]` line reports 2 and is wrong. Largest new module: `agent_cli` at 327 lines.
`oversized-file-config` (`#host-desktops` #659 had grown it from 1,557 to 1,575) is closed on this number.

**Public surface.** Seven items — `LocalConfig`, `KeyringConfig`, `SandboxCgroupConfig`, `GitConfig`,
`AgentWarmupConfig`, `SessionRoomConfig`, `AllowedTool` — have no path outside the crate any more; nothing
outside it used them. Every other path is unchanged. A `named` facade restores one.

In-crate files re-pointed to `config::access`: `first_login_enrolment`, `live_users`, `open_vault_idle_ttl`,
`pending_login_ttl`. Baseline held: `tddy-daemon-kernel` 154 passed / 0 failed before and after.

Docs: `docs/daemon-kernel.md` (module rules, `config` layout).
