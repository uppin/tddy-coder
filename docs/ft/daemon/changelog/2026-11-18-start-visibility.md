# 2026-11-18 — A failing start on a target says which command failed, and what it left behind

`GetSessionStart` lists `remote_calls`: every command a start ran on its target (an SSH host or a
Coder workspace), oldest first and at most 50, each with its host, command, duration, outcome
(`exited` with its exit code, `timed_out` or `unreachable`) and, for a call that failed, its stderr
tail — never a credential. Each call advances `updated_at_unix_ms`, so a slow command reads
differently from a hung one, and a one-shot past its deadline fails naming its command. `branch` and
`worktree` are recorded as soon as the start cuts its worktree and stay on the record when the start
fails or is cancelled. `DescribeSessionStartOptions` states that a derived host's `usable` means
*listed as startable*, not probed. See [project-concept.md § Project-home tools](../project-concept.md).
