# 2026-11-17 — A session whose agent ends says so

When the agent of a session's main terminal ends — it exited, crashed, or was stopped — the
daemon publishes one `ATTENTION_REQUIRED` notification with source `SESSION_ENDED`, text
`⏹ Session {label} ended: {outcome}` and a structured `ended { outcome, exit_code }`, so a
reader branches on the exit code rather than parsing prose. Because it is an attention
notification of a project's session, it lands in the project notification log, so an
orchestrator reading its inbox through `ReadProjectNotifications` learns that a session it
drives is gone. A tmux client ending while its agent lives raises nothing; the end is raised
later, when the detach watcher sees the tmux session go, reading `exit code unknown`. `/clear`
and a tab's end raise nothing, Telegram sends nothing for it, and the session drawer's dot
does not light for it.

See [Session notifications § What is notified](../session-notifications.md#what-is-notified).
