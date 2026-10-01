# 2026-10-01 — `subagent_resume` takes a `resetWorktree` choice and resets through the daemon

**Type:** Feature

`#agent-worktree` 2/4, PR [#561](https://github.com/uppin/tddy-coder/pull/561). Cross-package entry:
[2026-10-01-agent-worktree-rewind-reset.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-rewind-reset.md).

`subagent_resume` advertises and parses `resetWorktree` (boolean, default `true`; a non-boolean is
refused by name) through `reset_worktree_choice`. Each Managed conversation is built with a
`ConversationWorktreeResetPort` (`src/worktree_reset_port.rs`) that sends `Reset` through
`reset_conversation_worktree` and maps `{"reset": …}` to a `WorktreeReset`, `null` to no worktree and
an error body to a refused resume. Pinned by `subagent_resume_advertises_reset_worktree_as_a_boolean`
over the real stdio wire and the unit tests of both new modules.
