# 2026-10-25 — The start core records a start's orchestrator

**Type:** Feature

`#start-parity` 2/9 ([#751](https://github.com/uppin/tddy-coder/pull/751)).
`start_session_on_request` records `orchestrator_session_id` (and `orchestrator_daemon_instance_id`
when it names another daemon) in the new session's changeset on the daemon that created it
(`svc_start_session_core/orchestrator_record.rs`), and refuses an orchestrated `tool` start
(`start_request_checks::refuse_an_orchestrated_tool_start`, `FAILED_PRECONDITION`). The
per-session-type body is `start_here`. `classify_start_placement` and `managed_recipe_for` moved to
`placement.rs` (engine moves): `svc_start_session_core.rs` 563 → 486 production lines. Code issue
`complexity-svc-start-session-core-start-session-core`: 391 → 79 + 334 (split, both still over 60).
