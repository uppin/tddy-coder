# 2026-11-15 — `RenameSession`, and session names in the orchestrator's reports

**Type:** Feature

`project_home_lifecycle_tools/rename_session.rs`: `RenameSession` (`session_id`, `display_name`, `daemon_instance_id?` → `{session_id, display_name}`), a lifecycle target tool normalising the name before the port is asked; `ProjectHomeLifecyclePort::rename_session`. `display_name` on `StartSessionArgs` (offered in the schema), `ListedSession`, `SessionActivity` / `ActivityPage` and `StartLedgerRecord`; `StartedSessionDetails::listing` answers a `StartedSessionListing`. Code issue `complexity-start-session-args-schema-schema-offering`: 191 → 195 lines, open.

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
