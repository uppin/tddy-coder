# 2026-10-21 — The session host and session launch no longer name LiveKit

`#satellite` 13/13, the stack's last node. Starting, attaching to, resuming and deleting sessions,
opening a session's room and attaching an agent from another daemon behave as they did before. The
session host and the launch code reach rooms only through the daemon's extension ports; LiveKit is
an integration the daemon wires in, and no daemon library depends on it. See
[Session room](../session-room.md#configuration).
