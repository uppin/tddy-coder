# oversized-file: telegram_notifier.rs — the session watcher and its keyboard builders

**Location:** `packages/tddy-telegram-control/src/telegram_notifier.rs`
**Moved:** was `packages/tddy-session-lifecycle/src/telegram_notifier.rs` — relocated unchanged by #494 (`#carve` 8/11)
**Category:** oversized-file
**Detected:** 2026-09-22 by the `/pr-wrap` file-length gate on #494
**Metrics:** **1,246 production lines** (1,752 total; `#[cfg(test)] mod acceptance_unit_tests` opens at `:1247`) · budget 500 · **~2.5× over**
**Thresholds breached:** length 1246 > 500
**Restructure:** required — `extract_module --to_file` plus the function splits its three complexity records describe
**Status:** Open — **unclaimed**; pre-existing, and #494 grew it by 7 with the developer's consent (`docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-22 | 1,239 | in `tddy-session-lifecycle`, before #494 |
| 2026-09-22 | 1,246 | after #494 moved it — seven lines |

## What the tool found

Already about 2.5× the budget before #494 touched it.

**#494's contribution is seven lines**, all `rustfmt` rewraps: `crate::active_elicitation`,
`crate::elicitation`, `crate::telegram_tracked_session` and `crate::config` became
`tddy_telegram::…` and `tddy_session_lifecycle::config::…` paths, and the longer paths no longer
fit on one line (the `use` block at `:15`, and the calls at `:483` and `:610`). No logic changed.

The file holds three things:

- free helpers (`session_telegram_label`, `is_terminal_session_status`,
  `mask_bot_token_for_logs`, `:55–110`);
- `TelegramSessionWatcher` and its `impl` (`:112–1050`), five `with_*` constructors and the
  presenter-event policy, including `on_server_message` (`:834`, 214 lines), `on_metadata_tick`
  (`:247`) and `send_mode_changed_action_lines` (`:451`, 149 lines), each with its own complexity
  record;
- the keyboard and message-body builders (`document_review_keyboard` … `mode_changed_keyboard`,
  `:1051–1240`), which are free functions with no `self`.

## Why it matters here

Every Telegram message a session produces passes through `on_server_message`, and the policy for
all of them is in one `impl` of about 940 lines.

## What would close it

1. **Move-only:** the keyboard and body builders (`:1051–1240`, about 190 lines) into a
   `telegram_keyboards.rs`. They are free functions and the cheapest seam.
2. **Move-only:** the elicitation-delivery methods (`cache_*`, `register_elicitation_surface_for_chats`,
   `send_mode_changed_*`, `replay_*`, `deliver_mode_changed_elicitation_outbound`, `:339–833`) into a
   second `impl TelegramSessionWatcher` block in their own module.
3. **Decomposition:** `on_server_message` and `send_mode_changed_action_lines`, per their records.
   Steps 1 and 2 take the file to about 560; step 3 takes it under 500.

## Verified by hand

2026-09-22: counted production lines to the first `#[cfg(test)]` (`:1247`, which opens the test
module) on both sides of #494, and read the diff: every changed line is a path rewrite or its
rewrap.
