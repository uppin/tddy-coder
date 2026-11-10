# 2026-11-02 — The shared jail launcher

**Type:** Feature

`jail_launch.rs` `LaunchSessions::launch_jailed` (`JailSpec`, `JailedSession`, `EXEC_PLAN_FILE`, `mcp_server_env`) and `claude_jail.rs` `launch_jailed_claude`; the sandboxed claude-cli start and relaunch call it, the start carries `session_role`, and `refuse_what_main_cannot_honour` allows `sandbox`. Depends on `tddy-sandbox-recipes`. `launch_jail`, `JailLaunch`, `JailRunnerEnv` and `resolve_relaunch_binaries` removed. Cross-package: [agent-tools-claude-jail](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-jail.md).
