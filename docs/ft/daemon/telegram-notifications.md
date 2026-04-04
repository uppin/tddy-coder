# Telegram session notifications (daemon)

## Purpose

Operators receive short Telegram messages when a **coding session’s recorded status** moves from one value to another, for sessions that are **active** (tool process alive) and **in progress**. Messages identify the session with the **first two hyphen-separated segments** of the session id (for example `018f1234-5678` for a UUID-shaped id), so many concurrent sessions remain distinguishable in a chat.

## Configuration

Daemon YAML may include an optional top-level **`telegram`** block:

| Key | Type | Meaning |
|-----|------|---------|
| **`enabled`** | boolean | When false or omitted with other keys unset per loader defaults, Telegram sends are not performed. |
| **`bot_token`** | string | Bot API token from BotFather (required when the block is present and used for real sends). |
| **`chat_ids`** | list of integers | Recipients (Telegram chat ids); each qualifying transition results in one message per listed id. |

Unknown keys under **`telegram`** are rejected when the file uses the same **`deny_unknown_fields`** policy as the rest of **`DaemonConfig`**.

### Environment overrides (`.env` / shell)

After the YAML file is loaded, **`tddy-daemon`** merges optional process environment variables (for example values set in a repo-root **`.env`** that **`./web-dev`** loads). This avoids committing secrets in YAML.

| Variable | Meaning |
|----------|---------|
| **`TDDY_TELEGRAM_BOT_TOKEN`** | Bot API token. If there is no `telegram:` block in YAML, a block is created and **`enabled`** defaults to **`true`** unless **`TDDY_TELEGRAM_ENABLED`** is set. |
| **`TDDY_TELEGRAM_CHAT_IDS`** | Comma-separated integer chat ids (e.g. `-1001234567890,123456`). Requires an existing `telegram:` block in YAML **or** **`TDDY_TELEGRAM_BOT_TOKEN`**. |
| **`TDDY_TELEGRAM_ENABLED`** | Explicit **`true`** / **`false`** (also **`1`**/**`0`**, **`yes`**/**`no`**, **`on`**/**`off`**). When unset, a token supplied only via env for a **new** config block enables Telegram; merging a token into YAML does not force **`enabled`** on—set this variable to turn notifications on or off. |

## Message content

Each notification is plain text. It includes:

- A **short session label** derived from **`session_id`**: the first two segments split on **`-`**, joined with **`-`** (for example `018f1234-5678-7abc-8def-123456789abc` → **`018f1234-5678`**).
- A **human-readable transition**: previous status and new status after a change is detected.

## Behavior (library contract)

The **`tddy_daemon::telegram_notifier`** module provides:

- **`TelegramSessionWatcher`**: tracks last-seen status per session id. The **first** observation for an active session records a baseline and **does not** send a message. Each **subsequent** change in status triggers at most **one** send per configured chat id.
- **Inactive sessions** (process not alive per caller-provided flag): no sends; internal baseline state for that session is not advanced from these ticks.
- **Unchanged status** on successive ticks (including repeated **terminal** statuses such as **`completed`** or **`failed`**): no additional sends.
- **`send_telegram_via_teloxide`**: performs **`Bot::send_message`** via **teloxide** for production sends; failures surface as **`Result`** errors for the caller to log without panicking.

Secrets: full bot tokens do not belong in log lines; helpers return **masked** representations suitable for diagnostics.

## Integration surface

When the daemon spawns a **`tddy-coder --daemon`** session, it connects to the child’s gRPC **`PresenterObserver.ObserveEvents`** stream (see **`tddy-service`** proto) and maps **`ServerMessage`** events to Telegram text (state transitions, workflow completion, goal started, backend selected). Daemon startup and graceful shutdown also send short lifecycle messages when Telegram is enabled.

Callers that read **`.session.yaml`** (or equivalent) on an interval can still use **`TelegramSessionWatcher::on_metadata_tick`** with **`session_id`**, **`status`**, **`is_active`**, **`DaemonConfig`**, and **`TelegramSender`**. Technical detail lives in **[telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)**.

## Tests

Automated coverage includes unit tests for labels, terminal-status classification, token masking, and integration tests with a mock sender (no live Telegram network in CI).

## Related documentation

- **[telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)** — implementation reference (`tddy-daemon`).
- **[ConnectionService](../../../packages/tddy-daemon/docs/connection-service.md)** — session listing and metadata sources used elsewhere in the daemon.
- **[systemd-install.md](systemd-install.md)** — where **`daemon.yaml`** is installed in production.
