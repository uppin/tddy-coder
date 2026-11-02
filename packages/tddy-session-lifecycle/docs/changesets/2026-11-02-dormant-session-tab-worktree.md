# 2026-11-02 — A stopped session's tabs start in its recorded worktree

**Type:** Feature

`CliManagerTerminalRoster::start` no longer refuses a session with no running main terminal. It
starts the tab in `recorded_worktree(tddy_data_dir, session_id)`
(`connection_service/svc_terminal_ports/recorded_worktree.rs`) — the worktree
`tddy_core::repo_root_for_session` reads from `<data dir>/sessions/<id>` — and refuses with
`FAILED_PRECONDITION` naming the session when nothing is recorded, or `INVALID_ARGUMENT` for an id
that is not one path segment. See [module-layout.md](../module-layout.md) and
[Terminal Sessions](../../../../docs/ft/daemon/terminal-sessions.md#start--stop).
`#session-tabs` 1/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-dormant-tabs.md`.
