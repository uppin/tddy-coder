# 2026-09-05 — An attached session is connected, not "connected over LiveKit"

The session drawer used to report two different kinds of connected, one per wire, and only one of
them looked like a connection to the operator: a session its own daemon served directly showed no
connection state at all, because the handshake overlay existed only on the LiveKit path. Attaching
now produces a single **session connection**, and the drawer reports one connected state for it
whatever carries it — so that configuration gets a real handshake overlay and a real status instead
of a blank pane.

What a session can *do* is now a property of its connection rather than of a status string: tracks
and a participant roster where the wire carries them, calls only where it does not. Which terminal a
session renders follows from that, and nothing else in the drawer asks which wire it is on.

A session's connection is also something that can be closed, and something that is: leaving the
drawer releases every session it was holding, and re-attaching one releases the connection it
replaces. Round-trip time in the traffic strip is measured on the session's own connection rather
than by joining its room a second time.

Node 3 of the `optional-livekit` stack, on top of the host connections in
[#437](https://github.com/uppin/tddy-coder/pull/437) and the host directory in
[#438](https://github.com/uppin/tddy-coder/pull/438). Gating the media and presence surfaces on
capability follows in [#440](https://github.com/uppin/tddy-coder/pull/440), and folding the
terminal's own second join into the session connection in
[#441](https://github.com/uppin/tddy-coder/pull/441).

Feature [session-drawer.md](../session-drawer.md), technical
[session-connections.md](../../../../packages/tddy-web/docs/session-connections.md).
