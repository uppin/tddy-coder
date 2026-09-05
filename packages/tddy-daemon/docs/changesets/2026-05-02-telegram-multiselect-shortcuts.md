# 2026-05-02 — Telegram MultiSelect shortcuts

**Type:** Feature

**`telegram_multi_select_shortcuts`**, **`eli:mn:`** / **`eli:mr:`** (≤64-byte **`callback_data`**); **`MultiSelectShortcutElicitationMeta`** in **`TelegramSessionWatcher`**; **`telegram_bot`** gate + **`handle_elicitation_multi_select_shortcut`** → **`PresenterIntent::AnswerClarificationMultiSelect`**. Tests: **`telegram_multi_select_acceptance`**, **`telegram_concurrent_elicitation_integration`**. Feature docs: [telegram-session-control.md](../../../../docs/ft/daemon/telegram-session-control.md), [telegram-notifications.md](../../../../docs/ft/daemon/telegram-notifications.md), [daemon changelog](../../../../docs/ft/daemon/changelog/). Technical: [telegram-notifier.md](../telegram-notifier.md). (tddy-daemon, tddy-core, tddy-service, docs)
