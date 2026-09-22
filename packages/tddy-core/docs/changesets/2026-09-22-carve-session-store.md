# 2026-09-22 — The session storage layer and SQLite leave the crate

**Type:** Refactor · `#carve` 7/11, PR [#493](https://github.com/uppin/tddy-coder/pull/493)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-session-store.md`](../../../../docs/dev/changesets/2026-09-22-carve-session-store.md)

`atomic_file`, `error`, `output` and `session_actions` move to
[`tddy-session-store`](../../../tddy-session-store/README.md), and each old path is a
`pub use tddy_session_store::<module>::*;` facade. Every `tddy_core::{atomic_file, error, output,
session_actions}::…` path still resolves, and no consumer of those four was edited.
`session_actions/session_dir.rs` stays, because it reads `changeset.yaml` through `read_changeset`.
`session_action_jobs/runner.rs` stays too, reaching the storage crate's `#[doc(hidden)] pub`
`session_actions::runtime` across the boundary. The glob makes that module reachable as
`tddy_core::session_actions::runtime`, with seven functions that were crate-private before. None
of them is API.

`session_catalog/` moves to [`tddy-session-catalog`](../../../tddy-session-catalog/README.md) with
**no** facade, because one would bring `sqlx` back. Its two test binaries,
`session_catalog_acceptance` and `session_catalog_red`, moved with it. The manifest drops `sqlx`,
`regex` and `tddy-actions`, and keeps `jsonschema` for `session_action_pipeline.rs`. New:
`tests/session_store_shape.rs`, which pins the manifest shape of this crate and both new ones.

Closed `heavy-dependency-sqlx-session-catalog`. **0** files in `src` name `sqlx` (it was 4), and
`cargo tree -p tddy-core -i sqlx` matches no package (it was a direct dependency). 44 of this
crate's 52 transitive dependents no longer compile SQLite.
