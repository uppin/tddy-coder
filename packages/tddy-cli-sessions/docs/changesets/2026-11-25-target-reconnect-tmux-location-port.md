# 2026-11-25 — Every tmux run behind a TmuxLocation port (node 1/3)

**Type:** Architecture

`tmux_location.rs`: the `TmuxLocation` port — `session_lives` (`Err(TmuxUnreachable)` is never read as gone), `attach_client` (exactly one session, never created), `kill`; `HostTmux` over the host tmux helpers; `FakeTmuxLocation` for tests; `WatchBackoff` (10 s doubling to a 5 min cap, 15 min hold grace). `TmuxRun` carries a location; `DetachWatch`, `classify_ended_main` and the attach/re-attach paths are written against the port, with the existing host signatures kept as `HostTmux` delegates. A refused host probe now classifies `ClientOnly` and starts a watcher instead of dropping the session's state. Part of #1040 — see [module-layout.md](../module-layout.md).
