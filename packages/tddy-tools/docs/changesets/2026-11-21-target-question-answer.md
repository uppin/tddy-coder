# 2026-11-21 — A target agent's question is answered by its orchestrator

**Type:** Feature · **Stack:** `#target-parity` 6/7 (PR #974)

`session_hook::question_decision` removed and `session_hook::question_decision_json` no longer re-exported: the encoding lives in `tddy_session_activity::hook_decision`. Full record: [`docs/dev/changesets/2026-11-21-target-question-answer.md`](../../../../docs/dev/changesets/2026-11-21-target-question-answer.md).
