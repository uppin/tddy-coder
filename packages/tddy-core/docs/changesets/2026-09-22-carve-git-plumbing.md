# 2026-09-22 — The git plumbing leaves for `tddy-git`

**Type:** Refactor · `#carve` 6/11, PR [#492](https://github.com/uppin/tddy-coder/pull/492)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-git-plumbing.md`](../../../../docs/dev/changesets/2026-09-22-carve-git-plumbing.md)

`worktree.rs` drops from **1,606 production lines to 428**: it keeps the four functions that read a
session's changeset — `setup_worktree_for_session`, `…_with_integration_base`,
`…_with_optional_chain_base` and `resolve_persisted_worktree_integration_base_for_session` — and
re-exports [`tddy-git`](../../../tddy-git/README.md) with `pub use tddy_git::*;`. `ssh_exec` is a
facade over `tddy_git::ssh_exec`. Every `tddy_core::worktree::…` and `tddy_core::ssh_exec::…` path
still resolves; no dependent was edited. The glob adds five newly-public helpers to
`tddy_core::worktree` — they were private to the file before and are `pub` in `tddy-git` now.

Closed `squatting-git-plumbing-worktree` (1,606 → 428 production lines; `tddy-git` has 0 workspace
dependencies). The two `complexity-worktree-*` records are unchanged — those functions stayed.
