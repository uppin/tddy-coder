# 2026-10-22 — session-control DTOs and CLI spawns leave mod.rs and session_start.rs; an unwired exposure is not a warning

**Type:** Refactor, Fix

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `telegram_session_control/mod.rs` **557 → 412**: the DTOs to `types.rs` (151, glob-re-exported).
- `telegram_session_control/session_start.rs` **719 → 295**: the model-pick callbacks and the two CLI
  spawns to `cli_spawns.rs` (444), by `move_impl_members`.
- **Fix:** a Claude start whose terminal exposure answers `FailedPrecondition` — the null exposure of a
  daemon with nothing wired — no longer logs a warning; any other refusal still does (`not_exposed`).
  `tddy-rpc` moved from a dev-dependency to a dependency for `Status`/`Code`. Two unit tests pin the
  distinction. `spawn_telegram_claude_cli` is 216 lines (224 before; still over 60, pre-existing).
- Records closed and deleted: `oversized-file-telegram-session-control-mod.md` (557 → 412) and
  `oversized-file-telegram-session-control-session-start.md` (719 → 295).
