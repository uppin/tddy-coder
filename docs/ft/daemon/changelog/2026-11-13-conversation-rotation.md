# 2026-11-13 — Resume continues the conversation after `/clear`

After `/clear` in a Claude Code session, resuming it — including a project's main after auto-stop — continues the new conversation instead of the one the session started with.

- The session record keeps every conversation the session has had, newest last.
- A `/clear` inside an agent tab does not change the session's conversation.
- The behaviour is declared in the agent's YAML spec; agents that declare nothing are unchanged.

See [Claude CLI session § Resume after `/clear`](../claude-cli-session.md#resume-after-clear).
