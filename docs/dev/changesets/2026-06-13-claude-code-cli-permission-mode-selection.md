# 2026-06-13 — Claude Code CLI permission mode selection

**Type:** Feature

`StartSessionRequest.permission_mode` (proto field 14); `build_claude_argv` and `ClaudeCliSessionManager::start()` gain `permission_mode: Option<&str>`; `--permission-mode <mode>` in PTY argv (default `auto`); `tddy-tools pty-relay --permission-mode` CLI arg. 16 acceptance tests. Feature [claude-cli-permission-mode.md](../../ft/daemon/claude-cli-permission-mode.md); product [daemon/changelog/](../../ft/daemon/changelog/). (tddy-service, tddy-daemon, tddy-tools)
