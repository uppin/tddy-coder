# 2026-04-06 — Telegram: concurrent elicitation (one chat, active token)

- **Coordinator:** **`ActiveElicitationCoordinator`** maintains a per-chat FIFO queue of workflow sessions; the head session owns the **active elicitation token** for Telegram interactive surfaces.
- **Outbound:** **`TelegramSessionWatcher`** registers elicitation requests on **`ModeChanged`**; sessions that are not primary for a chat receive a **deferred** text notice without a competing full **`eli:s:`** inline keyboard.
- **Inbound:** **`telegram_bot`** applies the same **active-token** policy to **`eli:s:`**, **`eli:o:`**, **`eli:mn:`**, **`eli:mr:`**, and **`doc:`** callbacks; **`/answer-text`** and **`/answer-multi`** check the active session before **`PresenterIntent`** calls. **`telegram_session_control`** advances the queue after completion on select, Other follow-up, multi-select shortcuts, applicable document-review actions, and successful text/multi answers.
- **Observability:** Deep per-chat queues trigger a **warning** log at a fixed depth threshold.
- **Feature docs:** [telegram-session-control.md](../telegram-session-control.md), [telegram-notifications.md](../telegram-notifications.md). Package: [telegram-notifier.md](../../../packages/tddy-daemon/docs/telegram-notifier.md), [changesets/](../../../packages/tddy-daemon/docs/changesets/).
