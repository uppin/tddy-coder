# 2026-09-26 — A nested worktree is rooted at its own workspace

**Type:** Fix

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)), `543125a6`; cross-package
entry: [2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

`registry::workspace_root_for` kept the outermost ancestor holding a `Cargo.toml`, so a worktree at
`<main>/.worktrees/<name>` resolved to the main checkout, and the warm index daemon served it another
branch's tree. It now follows `cargo locate-project --workspace`: the nearest manifest declaring
`[workspace]` (or a `[workspace.*]` table), else the nearest manifest, else the directory, with the
walk stopping at the first directory holding `.git`. An unreadable manifest is `LspError::Io`. Tests:
`tests/workspace_root_test.rs`. `fake_lsp` also replays `workspace/didChangeWatchedFiles` as
`tddy/watchedFileChanges` (`841545dd`). See [workspace-root.md](../workspace-root.md).
