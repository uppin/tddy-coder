# 2026-07-02 — Daemon-hosted sandboxed sessions gain discovery-subagent + tool-replacement parity

**Type:** Feature

new `SubagentSpawnConfig`/`subagent_env_overlay`/`prepare_context_dir_with_subagent` (`sandbox_session.rs`); `start_sandboxed_claude_cli_session`, `relaunch_sandboxed_runner`, `resume_sandboxed_claude_cli_session`, and the `StartSession` dispatch all wired so a daemon-hosted session gets the same `TDDY_SUBAGENT_*`/allowlist-filtering/appendix behavior `tddy-sandbox-app` already had. Feature [managed-codebase-subagents.md § Tool replacement](../../../../docs/ft/coder/managed-codebase-subagents.md#tool-replacement-subagent-declared) (AC18). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon, tddy-discovery, tddy-core, tddy-service)
