# Telegram session control (daemon library)

## Purpose

The **`tddy_daemon::telegram_session_control`** module defines the **inbound** side of Telegram-driven workflow control: command and callback parsing, plan text chunking for Telegram message size limits, presenter input byte encodings aligned with the web client, and a **`TelegramSessionControlHarness`** used in automated tests (and suitable for future wiring from a teloxide update loop).

This surface is **distinct** from **[Telegram session notifications](telegram-notifications.md)**, which cover **outbound** status and elicitation hints from the daemon’s observer path.

## Library contents

| Area | Responsibility |
|------|----------------|
| **Commands** | **`parse_start_workflow_prompt`** extracts the prompt after a **`/start-workflow`** prefix. |
| **Callbacks** | **`parse_callback_payload`** recognizes recipe-style routing strings; recipe selection merges into **`changeset.yaml`** inside the harness. |
| **Chunking** | **`chunk_telegram_text`** splits UTF-8 text with a newline and **`(continued)`** suffix on non-final segments when the byte budget allows continuation markers. |
| **Presenter bridge** | **`map_elicitation_callback_to_presenter_input`** produces **`PresenterInputPayload`** bytes matching the web encoding for single- and multi-select elicitation. |
| **Persistence** | **`read_changeset_routing_snapshot`** reads **`recipe`**, **`demo_options`**, and **`run_optional_step_x`** from **`changeset.yaml`** for assertions. |
| **Harness** | **`TelegramSessionControlHarness`** creates a session directory under a configurable base path, sends an intro message with an inline recipe keyboard (test contract includes **`tdd-small`** labeling), applies recipe callbacks to **`changeset.yaml`**, sends plan review text in chunks via **`TelegramSender`**, and returns an explicit denial message for chat ids outside an allowlist. |
| **Test sender** | **`InMemoryTelegramSender`** (in **`telegram_notifier`**) stores optional **row-major** inline keyboard labels; **`drain_outbound_messages`** exposes structured **`CapturedTelegramMessage`** rows for tests. |

## Configuration and security (harness)

The harness takes an **allowed chat id list** and a **sessions base directory** supplied by the caller. Unauthorized chats receive a plain-text denial and do not create session directories. Full integration with **`DaemonConfig`**, bot tokens, and **`chat_ids`** in **`daemon.yaml`** belongs to the daemon binary when inbound control is enabled there.

## Operational scope

Inbound teloxide long-polling or webhook dispatch, durable **chat ↔ session** registry across restarts, and **`ConnectionService` / `StartSession`** wiring from Telegram are **outside** this library module; they remain follow-up work for a complete operator control plane.

## Tests

- **Unit tests** live in **`telegram_session_control.rs`** (`#[cfg(test)]`): parsers, chunking, presenter bytes.
- **Integration tests** live in **`packages/tddy-daemon/tests/telegram_session_control_integration.rs`**: start workflow, recipe **`changeset.yaml`**, plan chunk markers, elicitation mapping, unauthorized denial.

## Related documentation

- **[telegram-notifications.md](telegram-notifications.md)** — outbound Telegram notifications and **`TelegramSessionWatcher`**.
- **[ConnectionService](../../../packages/tddy-daemon/docs/connection-service.md)** — session lifecycle RPCs used by other clients (referenced for context; Telegram control does not replace these APIs in the current library scope).
