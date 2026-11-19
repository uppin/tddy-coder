# 2026-11-19 — A target session leaves nothing following its hook log after it ends

A session whose agent runs on a target no longer leaves a `tail -f` of its hook log running there
once it has ended: the follow ends within a second of the session's end removing the log. See
[agent-session-status.md](../agent-session-status.md) § Hooks on a target.
