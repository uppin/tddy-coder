# 2026-10-17 — The session host is a ReachableSessions and accepts a SessionReach

**Type:** Architecture

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

`DaemonSessionHost::with_session_reach` installs the extension (default `NoSessionReach`) and
`impl ReachableSessions for DaemonSessionHost` (`svc_reachable_sessions.rs`) gives it `resolve`,
`session_services` and `expose_terminal`. `ensure_session_room` remains for the agent-roster paths.
`tests/session_room_acceptance.rs` gains the local-connect, `OpenSessionRoom`, background-open and
no-plugin tests; the test fixture's `connect_to` stamps `RequestTransport::LiveKit`, and the test that
named the defect as the contract (`connecting_to_a_session_fails_when_livekit_cannot_be_reached`) is
replaced by `connecting_over_livekit_fails_when_livekit_cannot_be_reached` plus the local tests.

Docs: [session-service.md](../session-service.md), [module-layout.md](../module-layout.md).
