# 2026-11-02 — Jail sections on ToolDef

**Type:** Feature

`ToolDef.{home, files, secrets, policy}` (`#[serde(default)]`): `HomeSection::{Persistent { key }, Session}`, `FileItem::{SeedOnce, Mirror, Overlay}`, `SecretItem { env, source: SecretItemSource::{HostEnv, HostFile}, when: Option<RouteGuard> }`, `PolicySection { jit, fork, exec_paths, sysctl_read }` in `tool/{home,files,secrets,policy}.rs`. Cross-package: [agent-tools-claude-jail](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-jail.md).
