# 2026-10-03 — Code navigation in the session code explorer

**Type:** Feature

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574),
`feature/live-plan/code-navigation`, base `feature/live-plan/live-plans`. Product entry:
[2026-10-03-code-navigation.md](../../ft/web/changelog/2026-10-03-code-navigation.md).

The session code pane's read-only preview is navigable. The warm index answers `Definition`,
`References` and `Hover` (`code_index.proto`); `tddy-daemon` proxies them as the web-facing
`code_navigation.CodeNavigationService`, authorised through the worktree service's
`resolve_listed_worktree` (now `pub`) and dialled through `IndexDaemonRegistry::connect`; the web's
`CodeBlock` offers ctrl/cmd-click, hover and a references list on `.rs` files. Without an `index_daemon:`
section the service answers `FailedPrecondition`; there is no fallback to `tddy_lsp_executor`.

**Backlog resolved:** `2026-09-16-indexdaemonregistry-connect-has-no-caller` — `connect` now has a
production caller (`CodeNavigationServiceImpl`) and its success leg is covered by
`tddy-daemon/tests/code_navigation_acceptance.rs`. The entry is deleted.

**Backlog added:** `2026-10-03-code-navigation-grew-four-oversized-files` — `runtime.rs`, `build.rs`,
`SessionMainPane.tsx` and `SessionsDrawerScreen.tsx` grew by this PR's wiring; the split is deferred.

| Package | Entry |
|---|---|
| `tddy-index-daemon` | [code-navigation](../../../packages/tddy-index-daemon/docs/changesets/2026-10-03-code-navigation.md) — `Definition`, `References`, `Hover` |
| `tddy-service` | [code-navigation](../../../packages/tddy-service/docs/changesets/2026-10-03-code-navigation.md) — `code_navigation.proto` |
| `tddy-daemon` | [code-navigation](../../../packages/tddy-daemon/docs/changesets/2026-10-03-code-navigation.md) — the service, first `connect` caller |
| `tddy-worktree-service` | [code-navigation](../../../packages/tddy-worktree-service/docs/changesets/2026-10-03-code-navigation.md) — `resolve_listed_worktree` is `pub` |
| `tddy-web` | [code-navigation](../../../packages/tddy-web/docs/changesets/2026-10-03-code-navigation.md) — navigable `CodeBlock` |

Final measurements (code issues): `oversized-file-runtime` 1,631 lines (from 1,620), `build` in
`runtime.rs` 891 lines (from 880), `oversized-file-build` 709 (from 695) — all open, unclaimed.
