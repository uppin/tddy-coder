# 2026-05-02 — Telegram tracked session gate + chat traffic logs

**Type:** Feature

**`telegram_tracked_session`**: **`SharedTelegramTrackedSessionCoordinator`**, **`should_suppress_workflow_keyboards_for_session`**, **Enter** bind + elicitation replay, **QueuePromotionReplay** bypass; structured **`telegram_traffic`** / **`telegram_bot`** logs. **`send_mode_changed_*`**, **`telegram_session_control`** replay bridge. Tests: **`telegram_tracked_session_acceptance`**, harness binds in concurrent + multi-select suites. Feature docs: [telegram-notifications.md](../../../../docs/ft/daemon/telegram-notifications.md), [telegram-session-control.md](../../../../docs/ft/daemon/telegram-session-control.md), [daemon changelog](../../../../docs/ft/daemon/changelog/). Technical: [telegram-notifier.md](../telegram-notifier.md). (tddy-daemon, docs)
