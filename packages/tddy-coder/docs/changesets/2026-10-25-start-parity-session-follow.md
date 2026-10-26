# 2026-10-25 — A tool session records the question it asks

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). New `session_participant::pending_question`:
`pending_question_for_mode` (select, multi-select, free-text), `record_pending_question`, and
`spawn_pending_question_recorder`, which `run.rs`'s daemon mode starts beside the ACP transcript
writer to follow the presenter's `ModeChanged` events.

Tests: `tests/pending_question_from_mode.rs`.
