# 2026-04-06 — Telegram `/start-workflow`: branch/worktree intent

**Type:** Feature

**`telegram_session_control`**: **`CB_TELEGRAM_INTENT`**, **`parse_telegram_intent_callback`**, **`send_intent_pick_keyboard`**, **`handle_telegram_intent_callback`**; **`send_project_pick_keyboard`** (renamed from **`send_project_pick_after_recipe`**); optional **`TelegramWorkflowSpawn::projects_dir_override`** for tests. **`telegram_bot`**: intent dispatch before project pick; after recipe, intent keyboard before project list. Integration tests **`telegram_intent_*`**. Feature doc: [telegram-session-control.md](../../../../docs/ft/daemon/telegram-session-control.md). (tddy-daemon, docs)
