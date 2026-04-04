# Telegram notifier (`telegram_notifier`)

## Overview

The **`telegram_notifier`** module implements session status notifications using **teloxide** (Telegram Bot API) and the **`log`** crate. It contains pure helpers, an async send path, a **`TelegramSender`** trait for dependency injection, and **`TelegramSessionWatcher`** for transition detection.

## Configuration (`DaemonConfig`)

**`telegram`**: optional **`TelegramConfig`**:

- **`enabled`**: **`bool`** (default false).
- **`bot_token`**: **`String`** — required when the **`telegram`** block is present in YAML (serde deserialization).
- **`chat_ids`**: **`Vec<i64>`** — default empty; each id receives a copy of the message on a qualifying transition.

## Public API

| Item | Role |
|------|------|
| **`session_telegram_label(session_id)`** | Returns **`Some("seg0-seg1")`** when **`session_id`** splits on **`-`** into at least two parts; otherwise **`None`**. |
| **`is_terminal_session_status(status)`** | **`true`** for **`completed`** and **`failed`** (ASCII case-insensitive); used for classification and logging. |
| **`mask_bot_token_for_logs(token)`** | Returns a fixed-format string that does not embed the token (length-only metadata). |
| **`send_telegram_via_teloxide(bot, chat_id, text)`** | **`Requester::send_message`**; maps teloxide errors to **`anyhow::Error`**. |
| **`TelegramSender`** | Async trait: **`send_message(chat_id: i64, text: &str)`**. |
| **`TelegramSessionWatcher`** | Holds **`last_status: HashMap<String, String>`**. **`on_metadata_tick`** implements the baseline / transition / inactive rules documented in the module rustdoc. |

## Logging

Log target **`tddy_daemon::telegram`** carries **`info`** and **`debug`** lines for send dispatch, tick entry, baseline recording, unchanged status, and per-chat sends. **`marker_json`** emits **debug** trace lines keyed by marker id (development aid; safe to trim in later passes).

## Dependencies

**`teloxide`** (with **rustls**, default features off where configured in **`Cargo.toml`**) aligns with the workspace **Tokio** stack.

## Tests

- **Unit** (in-module): label extraction, terminal status, masking, inactive session behavior.
- **Integration** (**`tests/telegram_notifier.rs`**): disabled config (zero sends), single send on transition with label in body, no duplicate sends when terminal status repeats.

See **[changesets.md](./changesets.md)** for the wrapped changeset line.
