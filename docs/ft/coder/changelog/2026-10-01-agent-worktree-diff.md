# 2026-10-01 — Reading the diff between any two points of a subagent conversation

PRD: the `#agent-worktree` stack's third node ([#562](https://github.com/uppin/tddy-coder/pull/562)).

`subagent_diff { sessionId, from?, to? }` returns the unified diff of what a subagent conversation
changed, so a caller can look before it ends the conversation. `from` is exclusive and `to` inclusive,
as in git's `from..to`; omitted, they are the conversation's base and branch tip. The reply carries
the file and line counts for the whole range, the diff text capped at 64 KiB and cut at a line
boundary (`truncated` says so), and the short hashes the range resolved to. A binary file appears as
git's `Binary files … differ` line. A commit outside the conversation, one a rewind dropped, a `from`
that is not an ancestor of `to`, and a conversation that never made a mutating call are refused. It
changes nothing and answers while a turn runs. It is allowlisted in the sandbox recipes beside
`subagent_cancel`.

See [managed-codebase-subagents.md](../managed-codebase-subagents.md) § `subagent_diff` — read what a
conversation changed.
