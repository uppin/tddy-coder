# 2026-10-01 — `ConversationWorktree::diff` reads a range of the conversation branch

**Type:** Feature

`#agent-worktree` 3/4, PR [#562](https://github.com/uppin/tddy-coder/pull/562). Cross-package entry:
[2026-10-01-agent-worktree-diff.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-diff.md).

`diff(from, to)` (`src/diff.rs`) resolves `from` (default base) and `to` (default tip), refuses a
commit that is not the base or on `base..branch`, and a `from` that is not an ancestor of `to`, then
answers `ConversationDiff { from, to, files, lines, diff, truncated }`. The text is
`git diff --no-ext-diff --no-textconv from to` -- no `--binary`, so a binary change is git's
`Binary files … differ` line -- cut at the last whole line under `DIFF_TEXT_CAP_BYTES` (64 KiB); the
counts come from `--name-status` / `--numstat` over the whole range. `resolve_commit` and
`short_hashes` in `reset.rs` are `pub(crate)` for it. Pinned by `diff_acceptance` (9).
