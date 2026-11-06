# oversized-file: telegram_bot.rs — the bot's message and callback dispatch

**Location:** `packages/tddy-telegram-control/src/telegram_bot.rs`
**Category:** oversized-file
**Detected:** 2026-11-06 by the `/pr-wrap` file-length gate on #824 (`#agent-tools` 18/19)
**Metrics:** **798 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **+298**
**Thresholds breached:** length 798 > 500
**Restructure:** required — see *What would close it*
**Status:** Open — **unclaimed**; regressed 2026-11-06 (788 → 798 in #824), split deferred to the `#agent-tools` sweep node (n20) by the developer

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-06 | 788 → 798 | first record. 788 on `origin/feature/agent-tools/cli-dispatch-collapse` (#823) → 798 after `#agent-tools 18/19` (#824): **+10** — the `/start-agent` message branch (+15 in `telegram_message_handler`), the claude/cursor model-callback branches folded into one `tcm:` branch plus the `tsa:` agent pick (−6 in `telegram_callback_handler`) |

## What the tool found

Two functions carry most of the file: `telegram_callback_handler` (415 lines, its own record
`complexity-telegram-bot-telegram-callback-handler.md`) and `telegram_message_handler` (171 lines),
both chains of `if let Some(..) = parse_*(..)` dispatch arms, and both in
`crap-telegram-bot-handlers.md` as untested.

## Why it matters here

Every new Telegram command or callback adds an arm to one of the two handlers, so the file grows with
each Telegram feature even when the feature lives in `telegram_session_control/`.

## What would close it

- **Decomposition:** a dispatch table per handler — each arm becomes a function `(parsed payload,
  harness, bot) -> HandlerResult` in a `telegram_bot/` submodule grouped by flow (session control,
  elicitation, chaining) — which takes both handlers under 60 lines and the file under 500.
- Tests first for the arms that move, per `crap-telegram-bot-handlers.md`.

## Verified by hand

2026-11-06: counted production lines to the first `#[cfg(test)]` at the base and the head of #824;
handler lengths measured by brace matching.
