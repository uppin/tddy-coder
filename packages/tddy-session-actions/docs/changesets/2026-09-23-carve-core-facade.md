# 2026-09-23 — The crate is created from `tddy-core`'s session-action runners

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from what `#carve` 7/11 left in `tddy-core`: `session_actions/session_dir.rs` (with the glob over `tddy_session_store::session_actions`), `session_action_jobs` and `session_action_pipeline`. They sit above `tddy-changeset` because each reads `changeset.yaml`. Workspace dependencies: `tddy-changeset`, `tddy-session-store`, `tddy-task`. `jsonschema` lives here now. Log targets keep their `tddy_core::…` names. Three test binaries and one `complexity-*` record (unchanged) moved in. `tddy-core` re-exports it whole. See [architecture.md](../architecture.md).
