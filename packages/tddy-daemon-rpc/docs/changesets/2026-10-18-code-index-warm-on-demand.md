# 2026-10-18 — CodeSession; WatchCodeIndex follows the worktree
**Type:** Feature

- `code_session.rs` (+ `code_session/{ask, keep_alive, without_an_index}.rs`, new): the Code pane's
  bidirectional `CodeSession` — authorise `open`, warm, follow `WatchRoot`, hold hover / definition /
  references while `LOADING`, probe without waiting, cancel, re-warm on `IDLE`, keep the index alive
  while open, `NONE` for a non-Rust worktree.
- `code_navigation.rs`: `IndexChannelSource::{connect_if_running, keep_alive, keep_alive_every}`;
  `WatchCodeIndex` reads the session's worktree and follows `WatchRoot` without starting anything; unary
  answers carry `state`. Converters in `code_navigation/convert.rs`, the index calls in
  `code_navigation/position_query.rs`.
- `code_index_warmup.rs`: `SessionIndexProgress` removed; `warm_for_session` / `IndexWarmupObserver`
  trigger a warm and drain it (`drain_warm`); `CodeNavigationServiceImpl::with_index_progress` removed.

Docs: [architecture.md](../architecture.md#code-navigation). Cross-package entry:
[2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).
