# 2026-10-24 — Cancel a session start in progress

A streamed session start runs as a background task and names that task first, so it can be
cancelled. Cancelling it stops the start on every host involved. A running clone is killed, and no
later step begins. A worktree cut or agent spawn that was already running is allowed to finish, and
the session it made is then deleted. No session directory, worktree or partial clone is left, and
the stream ends `CANCELLED`. On a split start the codebase host's task is cancelled too. A
`CancelTask` addressed to another daemon is forwarded to it. See
[Background tasks § Session starts run as tasks](../background-tasks.md#session-starts-run-as-tasks)
and [Remote-managed worktree § Cancelling a split start](../remote-managed-worktree.md#cancelling-a-split-start).
