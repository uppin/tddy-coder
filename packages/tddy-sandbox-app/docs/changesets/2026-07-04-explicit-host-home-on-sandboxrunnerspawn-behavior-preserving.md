# 2026-07-04 — Explicit `host_home` on `SandboxRunnerSpawn` (behavior-preserving)

**Type:** Fix

`tddy-daemon`'s `build_sandbox_plan` no longer hardcodes `$HOME`; it reads a new `SandboxRunnerSpawn.host_home` field (so daemon sessions can pass `None` and disable the recipe's per-session credential copy for their persistent jail home). The `./claude-sandbox` app path now sets `host_home: Some($HOME)` explicitly, keeping its prior behavior unchanged. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#281](https://github.com/uppin/tddy-coder/pull/281). (tddy-sandbox-app, tddy-daemon)
