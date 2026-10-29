# 2026-10-29 — `AnswerSessionQuestion`

**Type:** Feature

`#home-inbox` 8/9 ([#773](https://github.com/uppin/tddy-coder/pull/773)).
`project_home_answer_tools.rs` gains `AnswerSessionQuestion` (`session_id`, `request_id`, `answers`,
`daemon_instance_id?`), `ProjectHomeAnswerPort::answer_question` (default refuses), and joins
`is_project_home_tool`, `is_project_home_target_tool` and `is_conversation_spawner_tool`.
