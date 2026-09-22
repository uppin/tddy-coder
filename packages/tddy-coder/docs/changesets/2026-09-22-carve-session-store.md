# 2026-09-22 — The worktree-open catalog populate names `tddy-session-catalog`

**Type:** Refactor · `#carve` 7/11, PR [#493](https://github.com/uppin/tddy-coder/pull/493)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-session-store.md`](../../../../docs/dev/changesets/2026-09-22-carve-session-store.md)

`tddy_core::session_catalog` no longer exists, and has no facade. `run.rs`'s
`spawn_session_catalog_populate` and `tests/session_catalog_populate.rs` name
`tddy_session_catalog`, and the crate depends on it directly. Behaviour is unchanged. This crate
still compiles `sqlx`, because it opens the catalog pool itself, which is why AC2's
`cargo tree … -i sqlx` check measures `tddy-workflow-recipes` and `tddy-tui` instead.
