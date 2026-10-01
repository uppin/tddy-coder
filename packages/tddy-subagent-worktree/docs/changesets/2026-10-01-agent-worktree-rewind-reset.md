# 2026-10-01 — `ConversationWorktree::reset_to` takes a conversation worktree back to the base or one of its commits

**Type:** Feature

`#agent-worktree` 2/4, PR [#561](https://github.com/uppin/tddy-coder/pull/561). Cross-package entry:
[2026-10-01-agent-worktree-rewind-reset.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-rewind-reset.md).

New `src/reset.rs`: `ResetTarget::{Base, Commit(abbreviation)}`, `WorktreeReset { to, dropped_commits:
Vec<String> }` (serialized `droppedCommits`, short hashes oldest first) and `reset_to`. It takes the
worktree lock, lists `base..branch`, refuses a commit that is not on it before anything moves, then
runs `reset --hard` and `clean -fd` (ignored files are kept). Mechanics:
[conversation-worktree.md](../conversation-worktree.md) § Reset. Covered by `reset_acceptance` (5).
The reset logic is split into `resolve_reset`, `resolve_commit` and `short_hashes` so `reset_to` stays
short.
