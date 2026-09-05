# 2026-06-13 — Telegram alerts for claude-cli session elicitations

**Type:** Feature

`TelegramSessionWatcher::on_claude_cli_activity_status_changed` sends a Telegram message to tracking chats on `WaitingForInput` and `Done` activity status transitions; `chats_tracking_session` reverse lookup on `TelegramTrackedSessionCoordinator`; hooked into `report_session_status`. Feature [telegram-notifications.md § Claude Code CLI session activity alerts](../../ft/daemon/telegram-notifications.md#claude-code-cli-session-activity-alerts). (tddy-daemon, docs)
