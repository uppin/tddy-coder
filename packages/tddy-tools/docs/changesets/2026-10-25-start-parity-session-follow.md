# 2026-10-25 — The session hook sends the agent's question

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). `session_hook::session_status_request`: the
`ReportSessionStatus` request a hook event becomes, carrying the event's `message` when it maps
to `WaitingForInput`. The two follow tools are admitted as project-home tools.

Tests: `tests/session_hook_pending_question.rs`.
