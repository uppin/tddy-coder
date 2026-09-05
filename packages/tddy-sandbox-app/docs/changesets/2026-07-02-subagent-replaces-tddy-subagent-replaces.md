# 2026-07-02 — `--subagent-replaces` + `TDDY_SUBAGENT_REPLACES`

**Type:** Feature

`SubagentSpawnConfig.replaces`; `subagent_env_overlay` sets the env var only when the flag was explicitly given (never invents the subagent's default, mirroring the `--fastcontext-*` fields' contract); the context-dir call site now passes the subagent name + effective resolved replaced set to `SandboxContextDir::create_with_subagent`. Feature [managed-codebase-subagents.md § Tool replacement](../../../../docs/ft/coder/managed-codebase-subagents.md#tool-replacement-subagent-declared). (tddy-sandbox-app, tddy-discovery, tddy-sandbox)
