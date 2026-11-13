# 2026-11-13 — Orchestrators drive a session's turns

**Type:** Feature

`#orchestrators 4/11` (PR [#881](https://github.com/uppin/tddy-coder/pull/881)). Cross-package: [session turn control](../../../../docs/dev/changesets/2026-11-13-session-turn-control.md). Product: [Sending a session a turn](../../../../docs/ft/daemon/project-concept.md#sending-a-session-a-turn).

- `ProjectNotificationRecord.orchestrator_session_id`, read from the session's changeset at append; the DTO field is `serde(default)` and omitted when `None`, so old lines replay.
- `ProjectNotificationScope.started_by` filters to one orchestrator's sessions.
- Docs: [session-notifications § Subscribers](../session-notifications.md#subscribers).
