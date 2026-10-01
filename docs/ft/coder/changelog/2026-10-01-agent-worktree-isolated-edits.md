# 2026-10-01 — A subagent edits its own worktree

PRD: the `#agent-worktree` stack's first node ([#560](https://github.com/uppin/tddy-coder/pull/560)).

A subagent that edits code no longer writes its caller's worktree. Its first mutating tool call cuts
an ephemeral branch and worktree from the caller's `HEAD` plus its uncommitted changes (one commit on
the new branch only); every mutating call that changes files is committed there, and its tool result
reports `worktreeChange` — files created, updated and removed, lines added and removed, the commit's
short hash. A new **`subagent_end`** applies the conversation's work to the caller's worktree as
uncommitted changes (3-way, conflict markers, the caller's `HEAD` never moves) and deletes the
worktree; **`subagent_cancel`** deletes it without handing anything back. A conversation that only
reads creates nothing.

Applies to the in-process subagent loop with Managed access. Daemon-run conversations, orphan
sweeping, reset on rewind, a diff tool and range pulls are not part of it. See
[managed-codebase-subagents.md](../managed-codebase-subagents.md) § The conversation worktree.
