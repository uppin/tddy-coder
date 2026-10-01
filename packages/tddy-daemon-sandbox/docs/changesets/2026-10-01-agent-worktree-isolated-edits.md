# 2026-10-01 — `DaemonToolHandler` runs a conversation's call in the conversation's worktree

**Type:** Feature

`#agent-worktree` 1/4, PR [#560](https://github.com/uppin/tddy-coder/pull/560). Cross-package entry:
[2026-10-01-agent-worktree-isolated-edits.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-isolated-edits.md).

The sandbox-session route, which never enters the exec-tool route, runs `execute` through
`execute_in_conversation` and `tddy_subagent_worktree::run_in_conversation`, so a mutating call of a
conversation lands in its worktree, is committed, and carries `worktreeChange`. A read before the
conversation's first write runs at the session root. Covered by the unit test
`a_conversations_call_runs_in_the_conversations_worktree`.
