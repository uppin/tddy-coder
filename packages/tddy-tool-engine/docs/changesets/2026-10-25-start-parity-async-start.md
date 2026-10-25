# 2026-10-25 — Start ledger, GetSessionStart and CancelSessionStart

**Type:** Feature

`#start-parity` 4/9 ([#753](https://github.com/uppin/tddy-coder/pull/753)). New `start_ledger`:
`StartHandle`, `StartState`, `StartStep`, `StartCloneProgress`, `StartLedgerRecord`,
`StartLedger::{in_session_dir, read, recover_interrupted}`, `begin_recorded_start(_with)` and the
`StartedSessionDetails` lookup. `project_home_tools`: `StartSession` answers with a handle, plus
`GET_SESSION_START` / `CANCEL_SESSION_START` and their `ProjectHomePort` methods.

Tests: `tests/start_ledger.rs`.
