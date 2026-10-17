# 2026-10-17 — session_room.proto and GetIntegrationStatus

**Type:** Feature

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

New `session_room.proto` (`SessionRoomService.OpenSessionRoom`) and, in `daemon_config.proto`,
`GetIntegrationStatus` with `IntegrationState`, `IntegrationStatus` and the request and response
messages. `build.rs` and `lib.rs` register the new module; the TypeScript bindings are regenerated.

Code issues: `oversized-file-build` 723 -> 737 production lines (+14); stays open, split deferred with the developer's consent.

Docs: [session-room-proto.md](../session-room-proto.md), [integration-status-proto.md](../integration-status-proto.md).
