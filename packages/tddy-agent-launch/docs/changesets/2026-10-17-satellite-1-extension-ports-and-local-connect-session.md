# 2026-10-17 — ConnectSession reports its reach through the SessionReach port

**Type:** Fix

`#satellite` 1/13 ([#623](https://github.com/uppin/tddy-coder/pull/623)); cross-package entry:
[2026-10-17-satellite-1-extension-ports-and-local-connect-session.md](../../../../docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md).

`ConnectSession` no longer opens the session room. For a session this daemon facilitates and which
records a checkout, it reports the reach (with the host-stamped transport the request arrived on)
through `LaunchSessions.session_reach`, and the port decides whether the caller waits. A local caller
therefore never waits on, or fails with, LiveKit. `LaunchSessions` gains `session_reach` and
`reachable_sessions`. `facilitated_checkout` / `FacilitatedCheckout` is the one "facilitated here over
a checkout" rule, shared with `ReachableSessions::resolve`.

Docs: [module-layout.md](../module-layout.md#connectsession-and-the-session-reach-port).
