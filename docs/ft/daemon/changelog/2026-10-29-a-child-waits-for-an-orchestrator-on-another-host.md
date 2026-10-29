# 2026-10-29 — A child waits for an orchestrator on another host

A claude-cli child whose orchestrator runs on a different daemon now waits for its answer to a
permission prompt or a question, as a same-host child does. The child's host asks the orchestrator's
host once, within 3 s, whether it serves the orchestrator's terminal; a "no", an error or silence
means no wait, and Claude's own prompt appears. The forwarded `AnswerSessionPermission` /
`AnswerSessionQuestion` wakes the hook on the child's host
([#787](https://github.com/uppin/tddy-coder/pull/787)). See
[claude-cli session § Waiting for an orchestrator's answer](../claude-cli-session.md#waiting-for-an-orchestrators-answer).
