# 2026-09-22 — The crate is created from `tddy-core`'s session catalog, and owns `sqlx`

**Type:** Refactor · `#carve` 7/11, PR [#493](https://github.com/uppin/tddy-coder/pull/493)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-session-store.md`](../../../../docs/dev/changesets/2026-09-22-carve-session-store.md)

Created from `tddy-core/src/session_catalog/`, where `mod.rs` becomes `lib.rs`: 854 lines and
the only four files in `tddy-core` that named `sqlx`. The `sqlx` dependency and its bundled SQLite
(`runtime-tokio`, `tls-rustls`, `sqlite`) move here, so only crates that open a catalog compile
SQLite. The schema, the queries and the populate behaviour are unchanged. The only code edits are
`crate::session_actions` becoming `tddy_session_store::session_actions` in three imports and three
doc links, `lib.rs` saying "this crate" where it said `tddy-core`, and one intra-doc link in
`provider.rs` becoming plain text because its target stays in `tddy-core`.

Workspace dependencies: `tddy-session-store` and `tddy-task`. It **never** depends on
`tddy-core`, which has no facade for it, and `tddy-core/tests/session_store_shape.rs` guards that.
`tests/session_catalog_acceptance.rs` and `tests/session_catalog_red.rs` moved here from
`tddy-core`, with their imports repointed. See [architecture.md](../architecture.md).
