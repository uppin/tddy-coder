# 2026-10-25 — ReportSessionStatus records the pending question

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). `service/hook_reports.rs`: a `WaitingForInput` report with a
`message` writes the session's `pending-question.json` (`kind: permission`); any other report,
or a wait with no message, clears it.

Tests: `tddy-session-lifecycle` `report_session_status_unit_tests.rs`.
