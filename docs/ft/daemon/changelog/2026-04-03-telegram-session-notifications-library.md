# 2026-04-03 — Telegram session notifications (library)

- **Config**: Optional **`telegram`** block in **`daemon.yaml`** with **`enabled`**, **`bot_token`**, and **`chat_ids`** (integer chat targets); unknown keys on the block are rejected under **`deny_unknown_fields`**.
- **Behavior**: The **`tddy_daemon::telegram_notifier`** module provides **`TelegramSessionWatcher`** (baseline + one notification per status transition for active sessions), **`session_telegram_label`** (first two hyphen segments of **`session_id`**), **`mask_bot_token_for_logs`**, and **`send_telegram_via_teloxide`** (teloxide **`Bot::send_message`**). Tests use a mock **`TelegramSender`**; CI avoids the live Telegram API.
- **Docs**: Product reference **[telegram-notifications.md](../telegram-notifications.md)**; technical reference **[telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)**.
