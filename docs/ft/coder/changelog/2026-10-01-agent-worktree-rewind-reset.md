# 2026-10-01 — Rewinding a subagent conversation resets its worktree

PRD: the `#agent-worktree` stack's second node ([#561](https://github.com/uppin/tddy-coder/pull/561)).

`subagent_resume { fromMessageId }` used to rewind the transcript and leave the files the dropped
messages had written. It now also resets the conversation's worktree to the commit of the last entry
the rewind keeps, or to the conversation's base when none made a commit; untracked files are removed
and ignored ones kept. Pass `resetWorktree: false` to rewind the transcript only. The turn outcome
reports `worktreeReset { to, droppedCommits }`, the dropped commits by short hash, oldest first. The
reset happens before the resumed turn's first model call, and a reset that fails refuses the resume
and leaves the conversation as it was. A conversation with no worktree resets nothing and creates
nothing.

Applies to the in-process subagent loop with Managed access; daemon-run conversations have no worktree
to reset. See [managed-codebase-subagents.md](../managed-codebase-subagents.md) § A rewind takes the
worktree back.
