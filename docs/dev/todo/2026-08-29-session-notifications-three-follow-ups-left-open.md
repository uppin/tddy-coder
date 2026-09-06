# 2026-08-29 — Session notifications — three follow-ups left open

**Category:** Future enhancement
**Source:** session-notifications-as-indicators changeset, 2026-08-29

- **The presenter's elicitation surface still bypasses the notification bus.** `ModeChanged`
  elicitations publish `ATTENTION_REQUIRED` for indicators, but the Telegram side of them keeps
  sending through `telegram_notifier`'s session-control path — inline keyboards, the per-chat
  elicitation FIFO (`ActiveElicitationCoordinator`) and the tracked-session gate have no
  representation on the bus, so the Telegram subscriber deliberately declines
  `source == PRESENTER` to avoid double-sending. Folding that path onto the bus means giving a
  notification an optional keyboard and a routing policy, which is a larger design than this
  changeset's plain-text events.
- **Two id-shortening rules still coexist in Telegram.** `session_telegram_label` (first two
  UUID segments) survives for inbound callback-data keys, alongside `claude-{id[..8]}` and
  `cursor-cli/{short_id}` in `telegram_session_control.rs`. Only the *notification* label was
  unified; the session list and chain-parent buttons still name sessions their own way.
- **`useSessionActivity.ts` is still unused.** It carries a per-session
  `unreadCount`/`markSeen` model over `StreamSessionActivity` and no component consumes it. The
  indicator work added a daemon-level feed rather than adopting it, because a per-session stream
  cannot drive a drawer of rows. Either it becomes the focused session's detail feed or it goes.
