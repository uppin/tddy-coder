# 2026-10-22 — Telegram on the integration status board

- The daemon settings screen shows a **Telegram** status line beside LiveKit's: **Off**,
  **Missing: bot_token**, **Connecting…**, **Connected · 3m** or **Unreachable · 12s: <error>**.
  A daemon that reports no Telegram entry shows no line. See
  [daemon settings § Integration status](../daemon-settings.md#integration-status).
- A misconfigured Telegram is visible: `enabled: true` with an empty `bot_token` reports
  **Misconfigured** instead of silently starting no bot. A revoked or wrong token shows as
  **Unreachable** after the daemon checks the bot's identity at startup, or after a failed send.
- No reported Telegram error contains the bot token.
- Telegram reaches the daemon's sessions through one operator channel. What an operator sees in a
  chat is unchanged, and the drawer's attention indicator works the same with or without a
  `telegram:` block ([telegram notifications](../telegram-notifications.md),
  [session notifications](../session-notifications.md)).
