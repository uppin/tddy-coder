# 2026-11-14 — The `#orchestrators` sweep: `session_hook` transport split, first subagent group out of `server.rs`

**Type:** Refactor

`#orchestrators` 12/12, PR [#901](https://github.com/uppin/tddy-coder/pull/901).

## What changed

- **B4** — `REPORT_TIMEOUT`, `deliver`, `HookTransport`, `hook_transport`, `exchange`,
  `call_over_socket` and `post_proto` moved from `session_hook.rs` to `session_hook::transport`,
  kept reachable with `reexport: "outside"`. `session_hook.rs` is 430 production lines.
- **B6 (partial)** — the first subagent group moved from `server.rs` to `server::subagent_tools`
  (296 lines) via `extract_module`. `server.rs` went from 2,289 to 2,024 production lines.

## Code issues

- `oversized-file-session-hook.md` deleted: 547 → 430 production lines (final measurement above).
- `oversized-file-server.md` kept open, narrowed to the remainder: 2,289 → 2,024 production
  lines. Accepted as standing debt by developer consent, 2026-11-14, reason "restructure engine
  refused" — see `docs/dev/todo/2026-11-14-restructure-engine-path-module-blocks-moves.md`.
