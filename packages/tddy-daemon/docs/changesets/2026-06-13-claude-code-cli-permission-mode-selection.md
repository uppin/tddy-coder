# 2026-06-13 — Claude Code CLI permission mode selection

**Type:** Feature

`build_claude_argv` gains `permission_mode: Option<&str>` (5th param); `ClaudeCliSessionManager::start()` gains `permission_mode: Option<&str>` (6th param); `--permission-mode <mode>` appended to PTY argv (default: `auto`; empty/whitespace normalised to `auto`); `StartSessionRequest.permission_mode` (proto field 14) wired through `connection_service::start_session → start_claude_cli_session → manager.start → build_claude_argv`; `tddy-tools pty-relay --permission-mode` optional CLI arg. Tests: `claude_cli_permission_mode_acceptance` (16 tests). Feature [claude-cli-permission-mode.md](../../../../docs/ft/daemon/claude-cli-permission-mode.md). (tddy-service, tddy-daemon, tddy-tools)
