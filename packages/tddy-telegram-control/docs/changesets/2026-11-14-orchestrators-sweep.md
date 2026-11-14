# 2026-11-14 — The `#orchestrators` sweep: telegram callbacks split

**Type:** Refactor

`#orchestrators` 12/12, PR [#901](https://github.com/uppin/tddy-coder/pull/901).

## What changed

- **B7** — the session-list items moved to `telegram_session_control::session_list_entries` and
  the recipe, intent, project, agent and branch callback parsers to
  `telegram_session_control::callback_data`, re-exported with `reexport: "glob"`.
  `callbacks.rs` is 405 production lines.

## Code issues

- `oversized-file-telegram-session-control-callbacks.md` deleted: 687 → 405 production lines
  (final measurement above).
