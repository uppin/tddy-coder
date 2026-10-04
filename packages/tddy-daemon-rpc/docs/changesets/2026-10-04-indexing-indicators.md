# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Cross-package entry:
[2026-10-04-indexing-indicators.md](../../../../docs/dev/changesets/2026-10-04-indexing-indicators.md). Product entry:
[2026-10-04-indexing-indicators.md](../../../../docs/ft/web/changelog/2026-10-04-indexing-indicators.md).

New `code_index_warmup.rs`: `SessionIndexProgress` (one `watch` channel per session; `follow` never
creates an entry), `warm_for_session` (background `code_index.Warm` through the `IndexChannelSource` port,
`None` when no index daemon or no `Cargo.toml` at the worktree root, a `Starting` record before the task
spawns, a failed or short warm ends with `error` set) and `IndexWarmupObserver`, the implementation of
`tddy-session-lifecycle`'s `SessionWorktreeObserver`. `CodeNavigationServiceImpl` gains `WatchCodeIndex`
and `with_index_progress`: it authorises with the worktree service's `resolve_owned_session_dir` (token →
OS user → `<sessions base>/sessions/<id>` must exist; a foreign and a missing session answer the same
`NotFound`) before any lookup. Docs: [architecture.md](../architecture.md#code-navigation).

**Decisions.** The warm lives here, not in `tddy-daemon`: its progress holder is read by the navigation
service in this crate, which cannot depend on the daemon, and `tddy-daemon/src` holds wiring only.
`warm_for_session` returns `Option<JoinHandle<()>>` so a test can await a warm that is otherwise
detached. Entries for warmed sessions are never freed.

**Not covered.** A session whose start does not announce its worktree (see tddy-session-lifecycle) is not
warmed; its index loads on its first navigation request. "Ready" is `Warm`'s `ready`; backlog entry
`2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph` (a reference, not resolved here) records
that it means a live server rather than a loaded graph.

**Tests (scoped).** The acceptance suite lives in `tddy-daemon` (it needs the registry). This crate's
code-issue records are not affected.
