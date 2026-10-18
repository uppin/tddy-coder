# 2026-10-18 — Per-root progress, WatchRoot, index state on navigation
**Type:** Feature

- `graph.rs`: `GraphLoad` publishes a `watch<RootProgress>` (latest-status `quiescent`, `loaded` latch,
  active phases, current, furthest, line) folded by an unthrottled `ServerChatter` for the server's
  whole life; `state_of_quiescence`. `GraphLoad::is_loaded` / `loaded()` removed.
- `watch_root.rs` (new): `WatchRoot` — never starts a server; `IDLE` / `LOADING` / `READY` /
  `FAILED` with `error` and every running phase; `RootSignals`; `next_change` / `Gone`; `reported`.
- `warm.rs`: `Warm` = `client_for` then forward the root's progress until loaded;
  `narrate_until_loaded` deleted.
- `navigation.rs`: `Definition` / `References` / `Hover` carry `IndexState` from the server's latest
  `serverStatus`.
- `plan_flush.rs` (new, private): the plan stores' debounced write-back, moved out of `index.rs` by the
  restructure engine.
- `lib.rs`: `is_rust_workspace`, `RUST_WORKSPACE_MANIFEST`.
- Proto: `WatchRoot`, `WatchRootRequest`, `IndexState`, `ActivePhase`, `IndexProgress.{state, phases,
  error}`, `state` on the three navigation answers.
- Tests: `root_progress_acceptance.rs` (7); `code_index_service_acceptance.rs` navigation tests warm the
  root first.

Code issues closed: `oversized-file-index` (index.rs **458** production lines, 522 on master) and
`complexity-warm-narrate-until-loaded` (function deleted). `poisoned-warm-latch-on-interrupted-index`
re-measured, unchanged.

Docs: [code-index-service.md](../code-index-service.md#root-progress). Cross-package entry:
[2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).
