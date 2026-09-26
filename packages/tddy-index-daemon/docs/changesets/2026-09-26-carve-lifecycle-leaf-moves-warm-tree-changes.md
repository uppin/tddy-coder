# 2026-09-26 — A warm server is told what changed on disk, and a nested worktree is served its own tree

**Type:** Fix

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

- **`543125a6`**: `WorkspaceIndex::workspace_root_of` resolves through `tddy-lsp`'s corrected
  `workspace_root_for`, so a request from `<main>/.worktrees/<name>` is served that worktree, not
  the main checkout; an unreadable manifest is `FailedPrecondition`.
- **`841545dd`**: `client_for` snapshots the root's `*.rs`, `Cargo.toml` and `Cargo.lock` files
  (new `tree_changes.rs`) and sends a server it already held `workspace/didChangeWatchedFiles` for
  every file created, changed or deleted since the previous request. A warm `check --deep` anchored in
  a module an earlier `apply` created had hung at 0% CPU for 20 minutes; it now sees the file. Tests:
  `tests/tree_changes_acceptance.rs` (4), unit tests in `tree_changes`.

See [code-index-service.md](../code-index-service.md#warm-state-per-workspace-root). The three code
issues (`warm.rs`, `apply.rs`) were not touched.
