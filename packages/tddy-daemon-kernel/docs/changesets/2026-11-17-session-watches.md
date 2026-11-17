# 2026-11-17 — Orchestrators watch the sessions they can see

**Type:** Feature · `#activity-wake` 2/3, PR [#930](https://github.com/uppin/tddy-coder/pull/930)

`SessionNotification.activity_status: Option<String>` carries the wire status an activity-status
notification was raised for, so the project notification log keeps it and a session watch can tell
a finished turn from a wait for input; every literal of the notification gains
`activity_status: None` outside the activity-status builder. Full record:
[`docs/dev/changesets/2026-11-17-session-watches.md`](../../../../docs/dev/changesets/2026-11-17-session-watches.md).
