# 2026-10-17 — LiveKit implements the session-reach port and reports its status

**Type:** Architecture

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

- `session_reach`: `LiveKitSessionReach` implements `SessionReach` (open the room now for a caller that
  arrived over LiveKit, in the background for everyone else) and serves `OpenSessionRoom` through
  `session_room_entry`; `livekit_integration_state` and `LIVEKIT_INTEGRATION_ID`.
- `OpenSessionRoom` resolves the session through `ReachableSessions::resolve`, so a session this daemon
  does not facilitate is refused with `failed_precondition`; a daemon with no LiveKit credentials
  answers `failed_precondition` too.
- `SessionRoomRegistry::ensure_open_staged` and `RoomOpenFailure::{Server, Elsewhere}`: only a failure
  to create or join the room on the LiveKit server reports `Unreachable`; a failure of the session's
  services, checkout or terminal bridge fails the request and leaves the status entry alone.
- The common-room discovery loop takes an `IntegrationReporter` and reports `Connecting`, `Connected`
  and `Unreachable`; retries keep `since`.
- `tests/dependency_boundary_unit.rs` reads `WorkspaceGraph`; `does_not_declare_the_daemon_as_a_dependency_of_its_own`
  replaces the test that read `[dev-dependencies]`, which the graph excludes by contract.
- New suites: `tests/livekit_integration_status.rs`, `tests/common_room_integration_status.rs`.
- Code issues: `oversized-file-session-room` 2,874 -> 2,935 production lines (+61) and
  `oversized-file-livekit-peer-discovery` 1,658 -> 1,697 (+39); both stay open, splits deferred with the developer's
  consent.
- Docs: [livekit-service.md](../livekit-service.md#livekitsessionreach--opening-the-room-for-a-reached-session), [session-room.md](../session-room.md).
