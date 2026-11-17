# 2026-11-17 — The log and the wire carry a session's end

**Type:** Feature · `#activity-wake` 1/3, PR [#929](https://github.com/uppin/tddy-coder/pull/929)

`streams::session_notification_event` maps the `SessionEnded` source and its `ended` part onto the wire; the project notification log stores the record as `source: "session_ended"` with an `ended` object (omitted when `None`), read back on replay, so the project-home inbox tools return a session's end. Full record: [`docs/dev/changesets/2026-11-17-session-ended-notice.md`](../../../../docs/dev/changesets/2026-11-17-session-ended-notice.md).
