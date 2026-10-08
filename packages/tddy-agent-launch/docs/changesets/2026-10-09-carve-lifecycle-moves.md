# 2026-10-09 — New crate: the agent launch

**Type:** Architecture

Created by `#carve` 21/21 ([#536](https://github.com/uppin/tddy-coder/pull/536)) from the launch topics of
`tddy-session-lifecycle`: session start and resume, the jailed and CLI-spawn starts, the relaunch, the stack,
child and conversation spawns, and the session coordinate handlers. 9,213 production lines in 52 modules,
46 tests; `LaunchSessions` and `trait LaunchHost` are defined here. `cursor_cli_spawn.rs` is 563 lines, moved
whole with its split deferred. Layout: [module-layout.md](../module-layout.md).
