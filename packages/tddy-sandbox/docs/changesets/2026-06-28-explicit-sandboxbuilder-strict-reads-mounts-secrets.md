# 2026-06-28 — Explicit SandboxBuilder + strict reads + mounts/secrets

**Type:** Feature

new `SandboxBuilder`→`SandboxPlan` (composition over `SandboxSpec`) with typed `ReadSpec`/`MountSpec`/`CopySpec`/`SymlinkSpec`/`SecretSpec`/`PolicySpec`/`NetworkSpec`/`ResourceLimits`; pure `build()` (no implicit lists); shared `materialize` helpers + `default_runner_env` (incl. `CLAUDE_CODE_TMPDIR`); single-source Claude recipe (`claude_required_reads`/`claude_required_copies` credentials-only/`claude_policy`). Architecture [architecture.md](../architecture.md); PRD [sandbox-builder.md](../../../../docs/ft/coder/sandbox-builder.md). (tddy-sandbox, tddy-sandbox-darwin, tddy-sandbox-cgroups, tddy-sandbox-runner, tddy-daemon, tddy-sandbox-app)
