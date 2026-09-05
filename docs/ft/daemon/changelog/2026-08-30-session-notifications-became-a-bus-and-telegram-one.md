# 2026-08-30 — Session notifications became a bus, and Telegram one subscriber on it

- **Telegram was not *a* notification surface, it was *the* notification system** — one method classified the event, rendered the copy, resolved recipients and sent, so a second consumer could not be added without reaching into it.
- **A session notification is now a published event** carrying `{session_id, label, kind, source, text, at_unix_ms, os_user}`; subscribers declare which kinds they want, and the copy is rendered once at publish so every surface reads the same sentence.
- **Telegram takes only attention-worthy activity-status events**, so chat traffic did not grow: `ACTIVITY` exists for indicators, and presenter elicitations keep their own keyboard-bearing surface.
- **Activity alerts name a session the way the web drawer does** — repo basename → workflow goal → short id — so a chat message and a drawer row are finally matchable. The other Telegram surfaces still use the short-id label.
- **`StreamSessionNotifications`** is a daemon-level feed: one subscription serves a drawer of any size, live-only, and scoped to the caller's OS user — the bus is host-wide, so the relay is the only thing between one operator and another's sessions.
- ⚠️ **Removed `TelegramSessionWatcher::on_claude_cli_activity_status_changed`** and its dedupe field; every behaviour they pinned is re-pinned against the new subscriber and the existing acceptance suite passes unmodified.
- **Known limitation:** a workflow session *started from Telegram* does not yet publish presenter events; web-started and resumed sessions do.
- See [session-notifications.md](../session-notifications.md).
