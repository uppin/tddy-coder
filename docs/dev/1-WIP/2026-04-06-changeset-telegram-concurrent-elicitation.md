# Changeset: Telegram concurrent elicitation (daemon)

Canonical product descriptions: **[telegram-session-control.md](../../../docs/ft/daemon/telegram-session-control.md)** (inbound), **[telegram-notifications.md](../../../docs/ft/daemon/telegram-notifications.md)** (outbound). Changelog: **[docs/ft/daemon/changelog.md](../../../docs/ft/daemon/changelog.md)**.

## Operator behavior (one chat, multiple workflows)

- **Single active token:** For each Telegram chat, the daemon keeps an ordered queue of workflow sessions that need elicitation. Only the **head** of the queue may show the primary interactive surface (full `eli:s:` / `eli:o:` inline keyboards where applicable).
- **Queued sessions:** Additional sessions get a deferred notice (no competing primary keyboard) until the active session completes its elicitation step.
- **Inbound routing:** Callbacks (`eli:s:`, `eli:o:`, `doc:*` document-review actions), plain-text “Other” follow-ups, `/answer-text`, and `/answer-multi` are honored only when the target session matches the **active** token for that chat; otherwise the user sees an alert and should use the web UI or wait.
- **Completion:** When the active session finishes the relevant step (select, Other follow-up, approve/reject document review, or text/multi answers as applicable), the queue advances and the next session becomes active.

## Code touchpoints

- `packages/tddy-daemon/src/active_elicitation.rs` — per-chat FIFO queue and `advance_after_elicitation_completion`.
- `packages/tddy-daemon/src/telegram_notifier.rs` — registers surface requests on outbound `ModeChanged`; defers keyboards when not primary.
- `packages/tddy-daemon/src/telegram_session_control.rs` — harness shares the coordinator; advances queue on completion paths.
- `packages/tddy-daemon/src/telegram_bot.rs` — authorizes callbacks and enforces the active-token gate for document review and elicitation callbacks.
