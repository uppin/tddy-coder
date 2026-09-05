# 2026-06-13 — Telegram alerts for claude-cli session elicitations

**Type:** Feature

**`telegram_notifier`**: `TelegramSessionWatcher::on_claude_cli_activity_status_changed` — alerts tracked chats on `WaitingForInput` / `Done` transitions (dedupe on same-status repeat); `last_activity_status` HashMap field; **`telegram_tracked_session`**: `chats_tracking_session` reverse lookup; **`connection_service`**: `report_session_status` calls `on_claude_cli_activity_status_changed` when Telegram configured. Tests: `telegram_claude_cli_activity_alert_acceptance` (5 new), inline unit tests for `chats_tracking_session` (2 new) and `on_claude_cli_activity_status_changed` (6 new). Feature [telegram-notifications.md § Claude Code CLI session activity alerts](../../../../docs/ft/daemon/telegram-notifications.md#claude-code-cli-session-activity-alerts). (tddy-daemon, docs)
