# 2026-04-04 — Session elicitation: Telegram `ModeChanged` + `ListSessions` flag

- **`connection.proto`**: **`SessionEntry.pending_elicitation`** (field **14**).
- **`tddy_core`**: **`SessionMetadata.pending_elicitation`** in **`.session.yaml`** (serde default **`false`**).
- **`tddy-daemon`**: Module **`elicitation`** — list flag from metadata; **`TelegramSessionWatcher::on_server_message`** handles **`ModeChanged`** with dedupe and generic approval/input Telegram lines; **`session_list_enrichment`** sets the proto field. Tests: **`telegram_notifier`** acceptance unit tests, **`list_sessions_enriched`**, **`session_list_enrichment`** unit test.
- **Feature docs**: [telegram-notifications.md](../telegram-notifications.md) (Presenter stream: elicitation); [web-terminal.md](../../web/web-terminal.md) (pending elicitation on rows). Package: [telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets/](../../../packages/tddy-daemon/docs/changesets/). Cross-package: **[docs/dev/changesets/](../../../dev/changesets/)**.
