# 2026-10-25 — The pending question and the uncapped activity read

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). New `pending_question`: `PENDING_QUESTION_FILE`
(`pending-question.json`), `PendingQuestion`, `write_pending_question` (temp file + rename),
`clear_pending_question` (idempotent), `read_pending_question`. `agent_activity`:
`read_all_agent_activity`, the coalesced read without the 500-call tail cap, which
`read_agent_activity` now caps. `session_activity::HookEvent` keeps the event's `message`.

Tests: `tests/pending_question.rs`.
