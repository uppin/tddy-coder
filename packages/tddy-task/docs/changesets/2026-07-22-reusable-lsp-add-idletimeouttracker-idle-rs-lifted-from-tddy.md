# 2026-07-22 — reusable-lsp: add `IdleTimeoutTracker` (`idle.rs`, lifted from `tddy-daemon::relay_idle`)

**Type:** Feature

`new`/`record_activity`/`should_shutdown` with unit tests; used by the LSP registry and re-exported by the daemon. Cross-package [changeset](../../../../docs/dev/changesets/). PR [#310](https://github.com/uppin/tddy-coder/pull/310).
