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
| **`TelegramSessionWatcher`** | Holds **`last_status`**, transition dedupe maps for stream events, **`last_elicitation_signature`** for **`ModeChanged`** dedupe, and **`active_elicitation`**: a **`SharedActiveElicitationCoordinator`** (see below) shared with **`telegram_session_control`** so outbound registration and inbound gating refer to the same per-chat queue. **`on_metadata_tick`** implements the baseline / transition / inactive rules; **`on_server_message`** maps **`ServerMessage`** variants including presenter **`ModeChanged`** via **`tddy_daemon::elicitation`**. For **`Select`** modes, registers full per-option confirmation strings in **`elicitation_select_options`** (shared with **`telegram_session_control`** for inbound confirmations). **`send_mode_changed_elicitation`** registers each elicitation session per configured chat, sends document/clarification chunks, then either attaches the full inline keyboard for the **primary** token holder or sends a deferred text-only notice when the session is queued. Mutex locks on the coordinator and select-option cache use explicit poison handling in production paths (errors are logged; sends degrade rather than panicking on poison). |
| **`ElicitationSelectOptionsCache`** | **`Arc<Mutex<HashMap<session_id, Vec<String>>>>`** — one confirmation string per option index for **`Select`** elicitation. |
| **`SharedActiveElicitationCoordinator`** / **`ActiveElicitationCoordinator`** | **`packages/tddy-daemon/src/active_elicitation.rs`**: per-chat **FIFO** of session ids awaiting elicitation; **`active_session_for_chat`**, **`elicitation_callback_permitted`**, **`register_elicitation_surface_request`**, **`advance_after_elicitation_completion`**. Helpers **`should_emit_primary_elicitation_keyboard`** decide whether the full **`eli:s:`** keyboard is attached for a given (**`chat_id`**, **`session_id`**) pair. |

## MultiSelect shortcut keyboards (`telegram_multi_select_shortcuts`)

Presenter **`MultiSelect`** clarifications add an inline shortcut row: **Choose none** uses **`eli:mn:<session_id>:<question_index>`**; **Choose recommended** uses a compact **`eli:mr:…`** payload and appears only when **`ClarificationQuestionProto.recommended_other`** is non-empty on the wire event. Each button’s **`callback_data`** stays within Telegram’s **64-byte** limit per key.

**`MultiSelectShortcutElicitationMeta`** (**session id**, **question index**, **`recommended_other`**) lives in **`TelegramSessionWatcher`**, keyed by Telegram chat id and session id, until the operator taps a shortcut or a newer elicitation supersedes it—**Choose recommended** forwards the cached **`recommended_other`** string as clarification **Other** without putting that text in **`callback_data`**.

**`send_mode_changed_elicitation`** applies the same **primary token** / **deferred text** rules as **`Select`**: the per-chat queue head attaches the shortcut row with the full **`MultiSelect`** surface; queued sessions receive the explanatory line without a competing primary keyboard.

Inbound **`telegram_bot`** routes **`eli:mn:`** / **`eli:mr:`** through the same **`authorized_elicitation_surface_gate`** as **`eli:s:`**, **`eli:o:`**, and **`doc:`**. **`handle_elicitation_multi_select_shortcut`** maps taps to **`PresenterIntent::AnswerClarificationMultiSelect`**: **Choose none** → empty indices and empty **Other**; **Choose recommended** → empty indices and the cached **`recommended_other`** as **Other**.

## Elicitation (`tddy_daemon::elicitation`)

| Item | Role |
|------|------|
| **`pending_elicitation_for_session_dir(session_dir)`** | Reads **`SessionMetadata.pending_elicitation`** from **`.session.yaml`** for Connection **`ListSessions`** enrichment (boolean; missing file → **`false`**). |
| **`elicitation_signature_for_mode_changed(mc)`** | Canonical string for deduplicating identical **`ModeChanged`** payloads per session. |
| **`telegram_elicitation_line_for_mode_changed(label, mc)`** | **`Some(line)`** for user-gated presenter modes (document review, markdown viewer, feature input, select/multi-select, text input); **`None`** for **`Running`** / **`Done`**. |

## Logging

Log target **`tddy_daemon::telegram`** carries **`info`** and **`debug`** lines for send dispatch, tick entry, baseline recording, unchanged status, and per-chat sends. **`marker_json`** emits **debug** trace lines keyed by marker id (development aid; safe to trim in later passes).

## Dependencies

**`teloxide`** (with **rustls**, default features off where configured in **`Cargo.toml`**) aligns with the workspace **Tokio** stack.

## Tests

- **Unit** (in-module): label extraction, terminal status, masking, inactive session behavior.
- **Integration** (**`tests/telegram_notifier.rs`**): disabled config (zero sends), single send on transition with label in body, no duplicate sends when terminal status repeats.

See **[changesets.md](./changesets.md)** for the wrapped changeset line.
