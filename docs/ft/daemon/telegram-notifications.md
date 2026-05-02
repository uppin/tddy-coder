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

When the daemon spawns a **`tddy-coder --daemon`** session, it connects to the child’s gRPC **`PresenterObserver.ObserveEvents`** stream (see **`tddy-service`** proto) and maps **`ServerMessage`** events to Telegram text (state transitions, workflow completion, goal started, backend selected, and presenter **`ModeChanged`** when the mode requires user input or approval—see **Presenter stream: elicitation** below). Daemon startup and graceful shutdown also send short lifecycle messages when Telegram is enabled.

## Presenter stream: elicitation (`ModeChanged`)

With Telegram enabled, **`TelegramSessionWatcher::on_server_message`** classifies **`ServerMessage`** **`ModeChanged`** payloads from **`PresenterObserver.ObserveEvents`**. Modes that require a human gate—document review, markdown viewer, feature input, clarification (**`Select`** / **`MultiSelect`**), and free-text **`TextInput`**—produce Telegram traffic per qualifying event. Autonomous modes **`Running`** and **`Done`** do not produce elicitation Telegram lines. Identical **`ModeChanged`** signatures per session id dedupe repeat sends so stream replays do not flood configured chats. Module **`tddy_daemon::elicitation`** centralizes classification, dedupe key material, and line templates; **[telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)** records the public hooks.

### Document review and markdown viewer

When the presenter asks the operator to **review / approve / reject** a document (document-review or markdown-viewer modes), Telegram must deliver:

1. **The full text of the document under review**, sent as one or more Telegram messages as needed (respect message size limits; reuse the same chunking approach as plan text in **`chunk_telegram_text`** where applicable).
2. **A follow-up message** carrying an inline keyboard with **Approve**, **Reject**, and **Refine** (labels must match the Virtual TUI / web semantics; if the product uses **View** / **Back** in some modes, those remain additional or alternate rows as defined by the presenter).

Formatting: adapt session markdown to what Telegram can render. The Bot API accepts **`parse_mode`** (e.g. MarkdownV2, HTML) or explicit **`entities`**; offsets and lengths for custom entities follow Telegram’s rules: lengths are measured in **UTF-16 code units** (not UTF-8 bytes or Unicode scalar counts), with supplementary-plane code points counting as two units. See **[Styled text with message entities](https://core.telegram.org/api/entities)** on [core.telegram.org](https://core.telegram.org/api/entities). Implementations should validate entity spans after conversion and split oversized content across messages when required.

**Security:** Do not paste unrelated secrets into chat; the requirement is to transmit the **review artifact** (plan/PRD text the workflow is asking the human to approve), not arbitrary environment or credential dumps.

### Clarification select (`Select`)

For **`Select`** clarification, the notification includes the usual short action line plus a **multi-line listing**: one line per option (full label; optional description on the same line). Inline keyboard buttons use **numeric labels** (1, 2, …) so the full text lives in the message body. After the operator chooses, the inbound Telegram path sends a **confirmation** message that repeats the **full** selected option text (label; description on a following line when present). The daemon keeps the per-option strings in a small in-memory cache keyed by session id so the confirmation matches the presenter without stuffing long text into **`callback_data`** (64-byte limit).

### Clarification multi-select (`MultiSelect`)

Outbound **`MultiSelect`** notifications append a shortcut row (**Choose none**, **Choose recommended** when **`recommended_other`** is non-empty on the wire proto) beside the usual short hint. **`callback_data`** uses **`eli:mn:<session_id>:<question_index>`** and **`eli:mr:…`** with a **≤64-byte** budget per button. **`TelegramSessionWatcher`** holds **`MultiSelectShortcutElicitationMeta`** (**session id**, **question index**, **`recommended_other`**) keyed by Telegram chat id plus session id until the shortcuts are tapped or superseded — the same **`recommended_other`** string is forwarded as clarification **Other** on **Choose recommended**. When the chat queue designates another session as primary, shortcut taps for a non-head session remain blocked at the inbound gate (**[telegram-session-control.md](telegram-session-control.md)**).

### Concurrent sessions in one chat (elicitation queue)

- **`ActiveElicitationCoordinator`:** A process-wide structure (shared with inbound **`telegram_session_control`**) records, per Telegram chat id, an ordered list of sessions that need elicitation surface. The list front is the session that may receive the **primary** inline keyboard for **`ModeChanged`** elicitation — **`Select`** (**`eli:s:`** / **`eli:o:`**), **`MultiSelect`** (**`eli:mn:`** / **`eli:mr:`** when present), document review (**`doc:`**), etc.
- **Registration:** Each outbound **`ModeChanged`** that represents user-facing elicitation registers the session for every configured **`chat_ids`** entry. Duplicate ids in the queue for the same chat are ignored.
- **Deferred surface:** When a session is registered but is **not** at the head of its chat queue, the notifier sends the action line as **text** (with a short “queued” explanation) and **does not** attach the full primary inline keyboard for that session (including **`Select`** numeric rows, **`MultiSelect`** **`eli:mn:`** / **`eli:mr:`** shortcuts, **`doc:`** review rows — whichever applies), so only the head session attaches a competing primary **`ModeChanged`** keyboard in the chat.
- **Depth monitoring:** When the per-chat queue length exceeds an internal threshold, the daemon emits a **warning** log line for operators (see **[telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)** and **`active_elicitation`**).

### Telegram-tracked session (keyboard gate)

Each configured Telegram **`chat_id`** holds an optional **tracked** workflow **`session_id`**: the session the operator chose to drive from Telegram for **workflow action** inline keyboards (elicitation **`eli:*`**, document review **`doc:*`**, plan / markdown approval rows, and related presenter **`ModeChanged`** surfaces).

- **Binding:** Established when the operator completes **Enter session** for that workflow session (same **`enter:<session_id>`** callback schema as the session list; see **[telegram-session-control.md](telegram-session-control.md)**).
- **Suppression:** When the chat has **no** tracked session, or the tracked session id **differs** from the outbound elicitation target, **`TelegramSessionWatcher`** sends the presenter action line with a single **Enter session** keyboard row instead of the full workflow keyboard. Plain text portions (document body, option listing in the message body) still follow the usual chunking rules; only **workflow action** inline keyboards are withheld under this policy.
- **Primary queue token unchanged:** The **active elicitation token** / FIFO rules in **`ActiveElicitationCoordinator`** still decide whether a session is **primary** for a chat; the tracked-session gate applies in addition when the session **is** primary and would otherwise attach a workflow keyboard.
- **Queue promotion replay:** Presenter **`ModeChanged`** sends classified as **queue promotion replay** bypass the tracked-session gate so promoted sessions attach their primary keyboards without requiring a second **Enter** after queue advancement.
- **Replay after Enter:** After binding, the daemon **re-sends** the pending presenter elicitation surface for that session (dedupe signatures align with intentional replay so the operator receives the full keyboard).
- **Clearing:** The association drops when the workflow reaches **WorkflowComplete** for that tracked pair, when **session delete** removes the tracked session, or when an explicit per-chat clear runs (leave / future control-plane hooks).
- **Traffic logs:** Structured lines on **`tddy_daemon::telegram`** describe outbound keyboard classes (**`workflow_keyboard`**, **`enter_only_keyboard`**, **`mode_changed`** routing) with **`chat_id`** and **`session_id`**. Inbound user text and callback routing use **`tddy_daemon::telegram_bot`** (and related helpers); **`callback_data`** appears only as a **short prefix** in logs—payloads remain compact opaque tokens. Full bot tokens never appear in logs (**`mask_bot_token_for_logs`** and related helpers).

### Other elicitation modes

For modes that are **not** full-document review (feature input, free-text **`TextInput`**), messages remain short hints: short session label and explicit **approval** or **input** wording. **`Select`** adds the numbered listing above; primary-queue **`MultiSelect`** adds the shortcut row described in **Clarification multi-select**; deferred-queue sessions omit competing primary keyboards while still emitting the queued explanatory text line.

Callers that read **`.session.yaml`** (or equivalent) on an interval can still use **`TelegramSessionWatcher::on_metadata_tick`** with **`session_id`**, **`status`**, **`is_active`**, **`DaemonConfig`**, and **`TelegramSender`**. Technical detail lives in **[telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)**.

## Tests

Automated coverage includes unit tests for labels, terminal-status classification, token masking, **telegram-tracked session** coordinator contracts, and integration tests with a mock sender (no live Telegram network in CI), including **`telegram_tracked_session_acceptance`** for gate, **Enter** replay, and log-shape assertions alongside existing concurrent-elicitation and multi-select suites.

## Telegram session control (library harness)

The **`tddy_daemon::telegram_session_control`** module implements parsing, chunking, **`changeset.yaml`** routing writes, presenter input bytes, and a **`TelegramSessionControlHarness`** for tests and future inbound integration. **`InMemoryTelegramSender`** stores optional inline keyboard labels for those tests. Inbound teloxide wiring and **`DaemonConfig`** flags for interactive control ship with the daemon binary when that path exists. Product reference: **[telegram-session-control.md](telegram-session-control.md)**.

## Related documentation

- **[telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md)** — implementation reference (`tddy-daemon`).
- **[ConnectionService](../../../packages/tddy-daemon/docs/connection-service.md)** — session listing and metadata sources used elsewhere in the daemon.
- **[systemd-install.md](systemd-install.md)** — where **`daemon.yaml`** is installed in production.
