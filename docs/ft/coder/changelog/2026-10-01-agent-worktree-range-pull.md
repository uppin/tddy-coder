# 2026-10-01 — Pulling a range of a subagent's commits

PRD: the `#agent-worktree` stack's fourth node ([#563](https://github.com/uppin/tddy-coder/pull/563)).

`subagent_pull { sessionId, from?, to? }` hands a chosen range of a subagent conversation's commits to
the caller's worktree while the conversation carries on, and `subagent_end` takes the same `from` /
`to`. Both bounds are inclusive; omitted, `from` is the earliest commit not yet pulled and `to` the
branch tip. Each commit is applied as its own 3-way apply, so a conflict names one commit. The
conversation remembers what it handed over: a commit pulled once is skipped by every later pull and by
`subagent_end`, and the reply lists `commits` and `skipped`. A rewind that drops commits already pulled
leaves them in the caller's worktree and names them in `worktreeReset.droppedPulledCommits`. A commit
not on the branch, a `from` after `to` and a pull while a turn runs are refused. `subagent_pull` is
allowlisted in the sandbox recipes beside `subagent_end`.

See [managed-codebase-subagents.md](../managed-codebase-subagents.md) § `subagent_pull` — take part of
the work now.
