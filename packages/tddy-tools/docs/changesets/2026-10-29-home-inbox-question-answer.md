# 2026-10-29 — `session-hook --await-answer` for `AskUserQuestion`

**Type:** Feature

`#home-inbox` 8/9 ([#773](https://github.com/uppin/tddy-coder/pull/773)). The await mode also
serves a `PreToolUse` of `AskUserQuestion`: `session_hook/await_answer.rs` (the shared
`AwaitQuestionAnswer` exchange, `question_decision_json`) and `session_hook/question_decision.rs`
(`encode_question_decision`, the `updatedInput` decision verified on Claude Code 2.1.312). Nothing
is printed unless the wait was answered and the answers fit the call's `questions`.
`session_hook.rs` stays under budget (490 → 498 production lines) by moving the await half out.
