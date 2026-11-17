# 2026-11-17 — A session's end lights no drawer dot

**Type:** Feature · `#activity-wake` 1/3, PR [#929](https://github.com/uppin/tddy-coder/pull/929)

`lib/sessionNotificationDot.ts`'s `dotForNotification`: attention lights yellow and activity blinks green as before, except a session's end (source `SESSION_ENDED`), which lights nothing — an ended session already reads as stopped. A kind this build does not know lights nothing either. Full record: [`docs/dev/changesets/2026-11-17-session-ended-notice.md`](../../../../docs/dev/changesets/2026-11-17-session-ended-notice.md).
