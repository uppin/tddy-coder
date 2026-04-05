# Telegram session control (daemon library)

## Purpose

The **`tddy_daemon::telegram_session_control`** module defines the **inbound** side of Telegram-driven workflow control: command and callback parsing, plan text chunking for Telegram message size limits, presenter input byte encodings aligned with the web client, and a **`TelegramSessionControlHarness`** used in automated tests (and suitable for future wiring from a teloxide update loop).

This surface is **distinct** from **[Telegram session notifications](telegram-notifications.md)**, which cover **outbound** status and elicitation hints from the daemon's observer path.

## Daemon binary (inbound)

When **`telegram.enabled`** is true, a non-empty **`bot_token`** is set, and the daemon can resolve **`sessions_base`** for the current OS user (`USER` → `~/.tddy`), **`tddy-daemon`** starts a **teloxide long-polling** dispatcher (`telegram_bot` module) alongside the web/RPC server. It handles **`/start-workflow`**, **`/sessions`**, **`/delete`**, and inline **Enter** / **Delete** / **More** callbacks (encoded as `enter:…`, `delete:…`, `more:…`). The same **`TeloxideSender`** / **`Bot`** instance is shared with outbound notifications.

## Commands (Updated: 2026-04-05)

| Command | Description |
|---------|-------------|
| **`/start-workflow <prompt>`** | Create a new session. Bot presents **Recipe: tdd-small** and **More recipes…**; **More recipes** sends a second message with **tdd**, **bugfix**, **free-prompting**, and **grill-me** (compact `mr:` callbacks). Choosing a recipe writes **`changeset.yaml`**. |
| **`/sessions`** | List sessions (10 at a time). Each session shows status, elapsed time, and workflow state. Paginated with a **"More"** inline keyboard button when more sessions exist. |
| **`/delete <session_id>`** | Delete a session. Bot sends SIGTERM/SIGKILL to the session process (if alive), removes the session directory, and confirms. |

### Session list behavior (`/sessions`) (Updated: 2026-04-05)

- Sessions are listed **most recent first**, 10 per page.
- Each entry shows: short session label, status, workflow state, elapsed time.
- Each entry has inline keyboard buttons: **"Enter"** (connect to session workflow) and **"Delete"** (delete session with confirmation).
- When more than 10 sessions exist, a **"More"** button at the bottom loads the next page (offset-based pagination via callback data).
- The list reads from the same **`session_reader::list_sessions_in_dir`** and **`session_list_enrichment`** pipeline as **`ConnectionService::ListSessions`**.

### Session deletion (`/delete`) (Updated: 2026-04-05)

- Delegates to the same **`session_deletion::delete_session_directory`** logic as **`ConnectionService::DeleteSession`**.
- If a live PID exists in **`.session.yaml`**, the daemon terminates the process before directory removal.
- Bot sends a confirmation message after successful deletion.

### Enter workflow (Updated: 2026-04-05)

- **"Enter"** button on a session row connects to that session's workflow.
- Bot presents the current workflow state and available actions (plan review, elicitation responses, etc.) using inline keyboards.
- Presenter input from Telegram uses the same **`map_elicitation_callback_to_presenter_input`** encoding as the web client.

### Document review on Telegram (Updated: 2026-04-05)

When a session presents a document for **review / approve / reject**, the operator must see the **same document body on Telegram** as in the Virtual TUI or web, not only a one-line “review in the UI” hint:

- Send the **full document text** first (split across messages if needed; **`chunk_telegram_text`** applies to plain segments).
- Then send **Approve**, **Reject**, and **Refine** as an inline keyboard row (or the product’s equivalent labels aligned with the presenter), matching **[telegram-notifications.md](telegram-notifications.md)** — presenter stream: elicitation, document review.

Markdown adaptation for Telegram must respect **[message entities](https://core.telegram.org/api/entities)** rules (notably **UTF-16–based** offsets and lengths for styled spans). Prefer Bot API **`parse_mode`** with validated markup, or build **`entities`** explicitly after converting from session markdown.

## Library contents

| Area | Responsibility |
|------|----------------|
| **Commands** | **`parse_start_workflow_prompt`** extracts the prompt after a **`/start-workflow`** prefix. |
| **Callbacks** | **`parse_callback_payload`** recognizes recipe-style routing strings; recipe selection merges into **`changeset.yaml`** inside the harness. |
| **Chunking** | **`chunk_telegram_text`** splits UTF-8 text with a newline and **`(continued)`** suffix on non-final segments when the byte budget allows continuation markers. |
| **Presenter bridge** | **`map_elicitation_callback_to_presenter_input`** produces **`PresenterInputPayload`** bytes matching the web encoding for single- and multi-select elicitation. |
| **Persistence** | **`read_changeset_routing_snapshot`** reads **`recipe`**, **`demo_options`**, and **`run_optional_step_x`** from **`changeset.yaml`** for assertions. |
| **Harness** | **`TelegramSessionControlHarness`** creates a session directory under a configurable base path, sends an intro message with an inline recipe keyboard (test contract includes **`tdd-small`** labeling), applies recipe callbacks to **`changeset.yaml`**, sends plan review text in chunks via **`TelegramSender`**, and returns an explicit denial message for chat ids outside an allowlist. |
| **Test sender** | **`InMemoryTelegramSender`** (in **`telegram_notifier`**) implements **`TelegramSender`** including **`send_message_with_keyboard`** (row-major **`(label, callback_data)`** per button); **`collect_outbound_messages`** exposes structured **`CapturedTelegramMessage`** rows for tests. |

## Configuration and security (harness)

The harness takes an **allowed chat id list** and a **sessions base directory** supplied by the caller. Unauthorized chats receive a plain-text denial and do not create session directories. Full integration with **`DaemonConfig`**, bot tokens, and **`chat_ids`** in **`daemon.yaml`** belongs to the daemon binary when inbound control is enabled there.

## Operational scope

The daemon binary runs **long-polling** inbound handling (see above). Durable **chat ↔ session** registry across restarts and full **`ConnectionService` / `StartSession`** automation from Telegram remain follow-up work where needed.

## Tests

- **Unit tests** live in **`telegram_session_control.rs`** (`#[cfg(test)]`): parsers, chunking, presenter bytes.
- **Integration tests** live in **`packages/tddy-daemon/tests/telegram_session_control_integration.rs`**: start workflow, recipe **`changeset.yaml`**, plan chunk markers, elicitation mapping, unauthorized denial.

## Related documentation

- **[telegram-notifications.md](telegram-notifications.md)** — outbound Telegram notifications and **`TelegramSessionWatcher`**.
- **[ConnectionService](../../../packages/tddy-daemon/docs/connection-service.md)** — session lifecycle RPCs used by other clients (referenced for context; Telegram control does not replace these APIs in the current library scope).
