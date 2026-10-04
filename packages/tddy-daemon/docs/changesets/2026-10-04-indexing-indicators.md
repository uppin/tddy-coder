# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Cross-package entry:
[2026-10-04-indexing-indicators.md](../../../../docs/dev/changesets/2026-10-04-indexing-indicators.md). Product entry:
[2026-10-04-indexing-indicators.md](../../../../docs/ft/web/changelog/2026-10-04-indexing-indicators.md).

`runtime.rs` builds the `IndexDaemonRegistry` before the session host, creates one
`SessionIndexProgress`, installs an `IndexWarmupObserver` on the host (`with_worktree_observer`) when
`index_daemon:` is configured, and hands the holder to the navigation service (`with_index_progress`).
`index_daemon/lsp_channel.rs` is unchanged. `tests/code_index_warmup_acceptance.rs` is new and registered
in `tests/test_placement.rs`. Docs: [code-navigation-service.md](../code-navigation-service.md),
[daemon-endpoint.md](../daemon-endpoint.md).

**Restructuring.** `unbundle_endpoint` lets `tddy-daemon/src` hold wiring only. The module this change
would have added to its list, `code_index_warmup.rs`, lives in `tddy-daemon-rpc`, and `index_daemon/` is
left as the parent has it; `tddy-daemon` gains no non-wiring module. An earlier plan to move the registry
cluster to a crate with the restructure engine was dropped; the engine defect it hit is backlog entry
`2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind` (open).

**Tests (scoped).** `code_index_warmup_acceptance` 6 passed, `code_navigation_acceptance` 7 passed,
`unbundle_endpoint` 4 passed, `test_placement` 4 passed; clippy `-D warnings` clean on the scoped
packages.

**File length (deferred).** `src/runtime.rs` 1,650 → 1,668 production lines (+18): code-issue record
`oversized-file-runtime` regressed; `complexity-runtime-build`: `build` 910 → 928 lines (+18) by brace
matching, record regressed. The split is deferred under the stack rule — dependent PR #572 and the
parents touch this file — and follows backlog entry `2026-10-03-session-lsp-tools-grew-runtime-rs`.
