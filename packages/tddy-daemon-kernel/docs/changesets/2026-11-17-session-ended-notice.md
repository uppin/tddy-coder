# 2026-11-17 — `SessionEnded`, a session's end as a notification source

**Type:** Feature · `#activity-wake` 1/3, PR [#929](https://github.com/uppin/tddy-coder/pull/929)

`SessionNotificationSource::SessionEnded`, `SessionEndNotice { outcome, exit_code }` and `SessionNotification.ended`, so a reader branches on how a session's agent ended without parsing the notification's text. Full record: [`docs/dev/changesets/2026-11-17-session-ended-notice.md`](../../../../docs/dev/changesets/2026-11-17-session-ended-notice.md).
