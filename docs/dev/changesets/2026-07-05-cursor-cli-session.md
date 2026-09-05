# 2026-07-05 — **cursor-cli-session

**Type:** Feature

Cursor Agent CLI session type** — `session_type = "cursor-cli"` spawns Cursor Agent CLI in a PTY worktree with `.cursor/hooks.json`, hook-driven `ReportSessionStatus`, web `CreateSessionPane` third type, Telegram `/start-cursor`, and `ListAgentModels("cursor-cli")`; `CliSessionManager` lives in `cli_session_manager.rs` (shim `claude_cli_session`). PRD [cursor-cli-session.md](../../ft/daemon/cursor-cli-session.md). Package indexes [tddy-daemon](../../../packages/tddy-daemon/docs/changesets/), [tddy-core](../../../packages/tddy-core/docs/changesets/), [tddy-tools](../../../packages/tddy-tools/docs/changesets/), [tddy-web](../../../packages/tddy-web/docs/changesets/). WIP source `docs/dev/1-WIP/2026-07-05-cursor-cli-session.md` removed after wrap. (tddy-daemon, tddy-core, tddy-tools, tddy-web)
