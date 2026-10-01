# 2026-10-01 — `ConversationWorktree::pull_range` applies a chosen range commit by commit

**Type:** Feature

`#agent-worktree` 4/4, PR [#563](https://github.com/uppin/tddy-coder/pull/563). Cross-package entry:
[2026-10-01-agent-worktree-range-pull.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-range-pull.md).

`pull_range(&PullRange, already_pulled)` (`src/range_pull.rs`) lists `base..branch` oldest first,
resolves `from` (default: the first commit not in `already_pulled`) and `to` (default: the tip), both
inclusive and both required to be on the branch, refuses a `from` after `to`, and applies each commit
not already pulled as `git diff --binary c^ c` through the scratch-index `apply --3way` of
`pull_into_caller` (`apply_3way` is now `pub(crate)`). It answers `RangePullOutcome { commits,
skipped, files, lines, conflicts }`; a range with nothing left is an empty outcome. A git failure
part-way leaves the commits applied so far in the caller's worktree. Pinned by `range_pull_acceptance` (9).
