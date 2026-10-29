# 2026-10-29 — AnswerSessionQuestion and the waiting AskUserQuestion hook

**Type:** Feature

`#home-inbox` 8/9 ([#773](https://github.com/uppin/tddy-coder/pull/773)). An orchestrated
claude-cli child's `AskUserQuestion` waits for its orchestrator's answers, as its permission prompt
does (`#home-inbox` 6/8). Across:

- `tddy-service` — `message QuestionAnswer {question_index, selected, other?}` and
  `AwaitQuestionAnswerResponse.answers = 6`; TS regenerated.
- `tddy-session-activity` — `pending_answers::question` (the question waiters),
  `answers::{answer_question, validate_question_answers}` with `AnswerRefusal::{NotAQuestion,
  InvalidAnswers, NoRequestId}`; `AwaitQuestionAnswer` branches on the pending question's kind.
- `tddy-agent-backend` — the `PreToolUse(AskUserQuestion)` hook entry runs `--await-answer` with the
  permission entry's timeouts.
- `tddy-tools` — the await mode matches `(event, tool_name)`; `session_hook/question_decision.rs`
  prints the `updatedInput` decision; the await half lives in `session_hook/await_answer.rs`.
- `tddy-tool-engine` / `tddy-daemon-rpc` — `AnswerSessionQuestion`, a target tool and a
  conversation spawner's tool; `DaemonProjectHome::answer_question`.
- `tddy-agent-launch` — the project-home prompt names the tool.

Decisions:

- **A separate tool** rather than widening `AnswerSessionPermission`, so no node changes a symbol
  another owns.
- **Mechanism: `updatedInput`**, verified interactively on Claude Code 2.1.312 (headless `claude -p`
  offers no `AskUserQuestion`): Claude skips its dialog and runs the tool as answered; a deny with
  the answers in its reason was not needed, and would leave a denied call in the transcript.
- **Await branch by kind**: `permission` keeps the permission path, anything else is a question.
  The hook's own dispatch is by event and tool, so a kind that disagrees with the hook prints
  nothing (an empty option id, or no answers, encodes to nothing).
- **Validation is against the question the waiter recorded**, not the pending file's, so what is
  checked is what the hook holds. After delivery the pending question is cleared only if it is still
  the answered one (hardened at validation: Claude may already have recorded its next).
- **Registry beside, not inside, the permission waiters**: `PermissionAnswer` has no room for
  per-question answers.
- **Recording**: `question:<request_id>` in the child's activity, beside `permission:<id>`.
- **No hook wait for any other `PreToolUse`**, and none for a `tool` session's clarification (no
  `request_id`), which is refused naming why.

Code issues: `exec_tool/project_home.rs` 842 → 889 and `project_home_tools.rs` 750 → 757 production
lines; splits deferred (stack-shared with `#home-inbox` 9/9), each record carries the measurement.
`tddy-tools/src/session_hook.rs` crossed budget (490 → 547) and was brought back to 498 by moving the
await half into `session_hook/await_answer.rs`.
