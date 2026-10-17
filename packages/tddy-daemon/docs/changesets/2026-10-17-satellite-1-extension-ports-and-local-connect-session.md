# 2026-10-17 — The daemon wires the LiveKit plugin, SessionRoomService and GetIntegrationStatus

**Type:** Architecture

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

`runtime::build` creates the `IntegrationStatusBoard`, the `LiveKitSessionReach` (installed with
`with_session_reach`) and registers the plugin's `session_room.SessionRoomService` entry.
`DaemonConfigServiceImpl::with_integration_status` serves `GetIntegrationStatus`, token-gated like
`GetConfig`, `failed_precondition` when no board is wired, with the wire mapping of
`IntegrationState` (error only for `UNREACHABLE`, missing fields only for `MISCONFIGURED`).
`spawn_common_room_discovery_task` keeps its signature and reports on a private board.

Code issues: `oversized-file-runtime` 1,781 -> 1,811 production lines (+30); stays open, split deferred
with the developer's consent.

Docs: [daemon-endpoint.md](../daemon-endpoint.md#integration-status-and-the-livekit-plugin).
