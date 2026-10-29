# 2026-10-29 — The project-home agent waits for its project's notifications

The project-home agent no longer polls `ListProjectSessions` to learn that a child finished, waits
for input, asks a question or a permission, or wrote it a notification.
`WaitProjectNotifications(cursor?, timeout_ms?)` holds for up to 20 s until one arrives, and
`ReadProjectNotifications(cursor?, limit?)` reads them without waiting. Both read a per-project log
of attention notifications kept by the daemon across restarts: the newest 1000 per project, with a
cursor that never repeats or skips a record and a `lost` count when it falls out of the window.
A pending question's record carries its `request_id` and content, ready for
`AnswerSessionPermission` ([#772](https://github.com/uppin/tddy-coder/pull/772)). See
[Project concept § Waiting for notifications](../project-concept.md#waiting-for-notifications) and
[Session notifications § The project notification log](../session-notifications.md#the-project-notification-log).
