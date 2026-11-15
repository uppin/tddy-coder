# 2026-11-15 — Session names in the listing, the Telegram label and the agent's own rename

**Type:** Feature

`session_reader::SessionEntry.display_name`; `resolve_session_label` returns the name before reading anything else; `RenameOwnSession` (`service/own_session_name.rs`) authenticated like `ReportOperatorNotification` (`operator_notify`, constant-time hook-token match, now shared as `hook_token_matches`). See [activity-service.md](../activity-service.md), [session-notifications.md](../session-notifications.md).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
