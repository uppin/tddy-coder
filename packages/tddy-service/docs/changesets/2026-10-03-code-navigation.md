# 2026-10-03 — `code_navigation.proto`: the web-facing navigation contract

**Type:** Feature

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574). Cross-package entry:
[2026-10-03-code-navigation.md](../../../../docs/dev/changesets/2026-10-03-code-navigation.md).

`proto/code_navigation.proto` defines `code_navigation.CodeNavigationService` (`Definition`,
`References`, `Hover`), keyed by session token, project id, worktree path and a worktree-relative
`rel_path` with a one-based line / byte-column position. Locations are `{rel_path, range,
outside_worktree}`. `build.rs` generates it with an RPC-server pass only, like `session_files.proto`,
and the TypeScript bindings go to `packages/tddy-web/src/gen/`. See
[code-navigation-proto.md](../code-navigation-proto.md).

Code issues: `oversized-file-build` grew 695 → 709 lines (one more generation pass in `main`), recorded
with a 2026-10-03 row; the split is deferred with the developer's consent. `complexity-service-acp-on-event`
names code this change did not touch.
