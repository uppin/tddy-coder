# 2026-10-29 — The project-home agent answers a session's permission prompt

A claude-cli session started by a project-home session (or a conversation spawner) no longer has to
wait for a person when Claude asks permission: its `PermissionRequest` hook holds for up to
`claude_cli.answer_wait_secs` (default 120 s), and the orchestrator answers with
`AnswerSessionPermission(session_id, request_id, option_id | cancelled, message?)` — `allow_once`,
`allow_always` when Claude suggested a rule, or `reject_once` with a reason the agent reads. Claude
proceeds as if a person had answered. Only the session that started the child may answer, the first
answer wins, and the decision is recorded in the child's activity. With no orchestrator running on
the child's host, or no answer in time, Claude shows its own prompt as before
([#771](https://github.com/uppin/tddy-coder/pull/771)). See
[Project concept § Answering a permission prompt](../project-concept.md#answering-a-permission-prompt)
and [Claude Code CLI session § Waiting for an orchestrator's answer](../claude-cli-session.md#waiting-for-an-orchestrators-answer).
