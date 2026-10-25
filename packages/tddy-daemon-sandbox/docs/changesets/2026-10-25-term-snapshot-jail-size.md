# 2026-10-25 — Jailed terminals' captures follow the jail's reported size
**Type:** Feature

`dial_and_bridge` receives the jail's terminal as ordered events, and
`sandbox_session::forward_jail_terminal_events` resizes the capture's screen model at the point in
the output stream where the jail reported it, so jailed terminals answer `SNAPSHOT` (#762,
`#term-snapshot` 3/4).
