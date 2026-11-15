# 2026-11-15 — Sessions can be named, by the operator, an orchestrator or the session's own agent

A session's name is recorded in its `.session.yaml`, listed as `SessionEntry.display_name`, and is its
label everywhere — Telegram activity alerts included — ahead of the derived one. It is set with
`SessionService.RenameSession` on the session's host, at a start (`StartSessionRequest.display_name`;
refused for a tool session type), by an orchestrator's `RenameSession` tool on any host, and by the
session's own agent with `tddy-tools rename-session`. `ListProjectSessions`, `ReadSessionActivity`
and `GetSessionStart` report it. See
[Session notifications § One label, everywhere](../session-notifications.md#one-label-everywhere) and
[Project concept § Project-home tools](../project-concept.md#project-home-tools).
