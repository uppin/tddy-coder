# 2026-10-25 — StartSession's arguments, schema and parity tables

**Type:** Feature

`#start-parity` 2/9 ([#751](https://github.com/uppin/tddy-coder/pull/751)). `start_session_args`:
`START_SESSION`, `StartSessionArgs` (`deny_unknown_fields`, proto field names),
`HostDocumentAttachment`, `CONTEXT_FILLED_FIELDS`, `INTERNAL_START_FIELDS`, `HostChoice`,
`StartChoices`, `StartContext`, `StartSessionArgs::into_request`; `start_session_args/schema.rs`:
`start_session_schema`, `start_session_schema_without_choices`, `offered_start_fields` (re-exported).
`project_home_tools`: `StartProjectSession` / `StartProjectSessionArgs` removed;
`session_offered_catalog(is_project_home, &StartChoices)`. Dev-dependencies `prost`, `prost-types`
(descriptor decoding in the parity test).

`start_session_args.rs` split by engine move: 540 → 257 + 292 production lines. Code issues: added
`complexity-start-session-args-schema-schema-offering` (178 lines); `oversized-file-lib` 877 → 878;
closed `oversized-file-project-home-tools` (500 → 435).

Tests: `tests/start_session_args_parity.rs`, `tests/start_session_schema.rs` (replacing
`tests/start_project_session_schema.rs`).
