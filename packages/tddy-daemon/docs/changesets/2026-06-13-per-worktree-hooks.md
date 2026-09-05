# 2026-06-13 — **Per-worktree hooks

**Type:** Feature

claude-cli session activity status** — **`connection_service`**: **`report_session_status`** RPC handler (path guard, `os_user` sessions_base, constant-time `hook_token`, `update_activity_status`); hook wiring in `start_claude_cli_session` (UUID token, writes `.claude/settings.local.json`); **`ClaudeCliSessionManager`** injected as constructor param; `build_claude_argv`/`PtyHandle::resize()`; **`session_list_enrichment`** surfaces `activity_status`; **`config`**: `tddy_tools_path`/`daemon_url` on `ClaudeCliConfig`. Feature [claude-cli-session.md](../../../../docs/ft/daemon/claude-cli-session.md#session-activity-status-via-per-worktree-hooks). (tddy-daemon)
