# 2026-09-26 — Roots follow tddy-lsp's workspace-root rule, and report an unreadable manifest

**Type:** Fix

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)), `543125a6`; cross-package
entry: [2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

Both calls of `tddy_lsp::registry::workspace_root_for` take its `Result`: a nested worktree is
rooted at its own Cargo workspace, bounded by `.git`, and an unreadable manifest on the walk is
returned as the executor's string error. See
[tddy-lsp's workspace-root.md](../../../tddy-lsp/docs/workspace-root.md).
