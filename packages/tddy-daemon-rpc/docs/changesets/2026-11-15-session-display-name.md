# 2026-11-15 — `RenameSession` and session names answered on this host

**Type:** Feature

`ProjectHomeLifecyclePort::rename_session` for `DaemonProjectHome`; `ListProjectSessions` rows and `ReadSessionActivity` carry the record's name. `rpc_handlers_acceptance`'s idle-tracker fixture no longer races a 1 ms timeout (`IdleTimeoutTracker::last_active_at`).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
