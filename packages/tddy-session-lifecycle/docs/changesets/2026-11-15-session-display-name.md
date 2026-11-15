# 2026-11-15 — `RenameSession` and `RenameOwnSession` through the session host

**Type:** Feature

`SessionHandler::rename_session`, forwarded by `SessionServiceImpl`, `DaemonSessionHost` (both of its `SessionService` impls) and `TestDaemon`; `PeerRoutedActivity` delegates `RenameOwnSession` locally, like `ReportOperatorNotification`. See [session-service.md](../session-service.md).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
