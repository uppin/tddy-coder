# 2026-09-23 — `git_plumbing_shape` follows the session-aware worktree layer to `tddy-session-worktree`

**Type:** Refactor · `#carve` 12/14, PR [#522](https://github.com/uppin/tddy-coder/pull/522)
Cross-package entry: [`docs/dev/changesets/2026-09-23-carve-core-facade.md`](../../../../docs/dev/changesets/2026-09-23-carve-core-facade.md)

The #carve 6 shape suite read `tddy-core/src/worktree.rs` by path and asserted that the session-aware layer stayed in `tddy-core`. #522 moved that layer, unchanged, to `tddy-session-worktree/src/worktree.rs`. The two tests now read it there and assert that it stays with the session model. They are renamed `the_session_worktree_module_*`. No production code changed.
