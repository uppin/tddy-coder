# 2026-10-29 — The project-home agent answers a session's question

A claude-cli session started by a project-home session (or a conversation spawner) no longer has to
wait for a person when Claude asks a question with `AskUserQuestion`: its hook holds for up to
`claude_cli.answer_wait_secs` (default 120 s), and the orchestrator answers with
`AnswerSessionQuestion(session_id, request_id, answers: [{question_index, selected, other?}])`.
Claude skips its dialog and continues as if a person had answered. The answers are checked against
the question as asked (a question index, an offered label, one answer to a single choice), only the
session that started the child may answer, the first answer wins, and the answers are recorded in
the child's activity. With no orchestrator on the child's host, or no answer in time, Claude shows
its own dialog as before ([#773](https://github.com/uppin/tddy-coder/pull/773)). See
[Project concept § Answering a question](../project-concept.md#answering-a-question) and
[Claude Code CLI session § Waiting for an orchestrator's answer](../claude-cli-session.md#waiting-for-an-orchestrators-answer).
