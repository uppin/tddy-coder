# 2026-11-15 — Renaming a session, and naming one at its start

**Type:** Feature

`svc_rename_session.rs` answers `RenameSession`. A start's `display_name` is validated before routing (`start_request_checks::requested_display_name`), a named `tddy_coder` start is refused (`refuse_a_named_tool_start`), and every other local start records the name once it has its session id (`record_the_started_sessions_name`). The listing carries the record's name, `StartedSessionListings` answers a `StartedSessionListing` with it, and `agent_notify_guidance.md` names `tddy-tools rename-session`. See [module-layout.md](../module-layout.md).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
