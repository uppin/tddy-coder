# 2026-04-06 — Telegram concurrent elicitation

**Type:** Feature

**`active_elicitation`**: per-chat FIFO queue and **`advance_after_elicitation_completion`**; **`telegram_notifier`** registers **`ModeChanged`** surfaces and defers non-primary **`eli:s:`** keyboards; **`telegram_session_control`** shares **`SharedActiveElicitationCoordinator`** with the watcher, advances on completion paths, gates **`/answer-text`** / **`/answer-multi`**; **`telegram_bot`** centralizes authorize + **`elicitation_callback_permitted`** for **`eli:s:`** / **`eli:o:`** / **`doc:`**. Integration tests: **`telegram_concurrent_elicitation_integration`**. Feature docs: [telegram-session-control.md](../../../../docs/ft/daemon/telegram-session-control.md), [telegram-notifications.md](../../../../docs/ft/daemon/telegram-notifications.md), [daemon changelog](../../../../docs/ft/daemon/changelog/). Technical: [telegram-notifier.md](../telegram-notifier.md). (tddy-daemon)
