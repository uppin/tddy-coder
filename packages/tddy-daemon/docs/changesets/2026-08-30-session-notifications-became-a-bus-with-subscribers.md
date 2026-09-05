# 2026-08-30 — session notifications became a bus with subscribers

**Type:** Feature

`ReportSessionStatus` published to `SessionNotificationBus` instead of calling Telegram inline, `TelegramNotificationSubscriber` became one consumer among several (taking only attention-worthy activity-status events, so chat traffic is unchanged but for the label), `StreamSessionNotifications` was added as a daemon-level per-user-scoped feed for the web's drawer indicators, and activity alerts now name a session with the shared `session_display_label` rule instead of its uuid prefix; removes `on_claude_cli_activity_status_changed`. See [session-notifications.md](../session-notifications.md).
