# 2026-09-23 — The crate is created from `tddy-core`'s session worktree layer

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

Created from `tddy-core`'s `worktree.rs`, `base_sync.rs`, `session_chain.rs` and `git_head.rs`, moved with `git mv`. Workspace dependencies: `tddy-changeset`, `tddy-session-store`, `tddy-git`. `worktree` still re-exports `tddy_git::*`. Ten test binaries moved with it. Two `complexity-*` records moved in, both unchanged. `tddy-core` re-exports it whole. See [architecture.md](../architecture.md).
