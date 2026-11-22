# 2026-11-21 — An orchestrator reaches each child by its best route and releases a terminal it claimed

`#orchestra 7/9` ([#985](https://github.com/uppin/tddy-coder/pull/985)).

- **`ListProjectSessions` says how to message each session** (`delivery_route`): `terminal` for a
  Coder/SSH child, `native` with its peer name for a local Claude child of a Claude orchestrator,
  `inbox` otherwise.
- **Cursor and Muse take `SendSessionMessage` turns**, pasted into their TUI and submitted with Enter.
- **`ReleaseSessionTerminal`** gives back a terminal the orchestrator claimed, so a person takes
  control without stealing; a session's claims are dropped when it ends.

See [Project concept § Sending a session a turn](../project-concept.md#sending-a-session-a-turn).
