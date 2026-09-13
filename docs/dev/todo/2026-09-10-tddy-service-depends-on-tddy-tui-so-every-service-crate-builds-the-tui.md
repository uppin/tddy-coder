# 2026-09-10 — `tddy-service` depends on `tddy-tui`, so every extracted service crate builds the TUI

**Category:** Future enhancement
**Source:** `#unbundle` node 4, [#473](https://github.com/uppin/tddy-coder/pull/473)

`tddy-service` carries the generated protos and `ServiceEntry`, so every crate the `#unbundle` stack
extracts depends on it — and `tddy-service` depends on `tddy-tui`. `tddy-daemon-livekit` and
`tddy-daemon-auth` therefore pull the terminal UI into their builds and their test binaries, for
nothing either of them uses.

The cost grows with the stack: each new service crate pays the same compile time, and a crate whose
whole point is a narrow dependency surface has a TUI on it.

Find what in `tddy-service` reaches `tddy-tui` and either move that piece out or put it behind a
feature the service crates do not enable.
