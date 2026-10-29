# 2026-10-29 — `answer_question` and the question waiters

**Type:** Feature

`#home-inbox` 8/9 ([#773](https://github.com/uppin/tddy-coder/pull/773)). `AwaitQuestionAnswer`
holds an `AskUserQuestion` as it holds a permission and returns the per-question `answers`
(`QuestionAnswer`, field 6 of the response); `answers::answer_question` and
`validate_question_answers` are what `AnswerSessionQuestion` runs. See
[activity-service.md § Holding a hook for an orchestrator's answer](../activity-service.md#holding-a-hook-for-an-orchestrators-answer).
