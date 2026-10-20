# 2026-10-20 — A start waits for its bridge's report

**Type:** Fix

`#host-desktops` 6/8 ([#658](https://github.com/uppin/tddy-coder/pull/658)). `bridge_readiness.rs` is new (`await_bridge_readiness`,
`BridgeStarted`, `DEFAULT_BRIDGE_READY_TIMEOUT` = 30 s, `with_bridge_ready_timeout`). `spawn_bridge`
pipes the bridge's stdout and waits for its report, and is split into `start_bridge_process`,
`record_bridge_pid`, `await_bridge_started`, `stop_failed_bridge`, `reap_bridge` and
`forget_bridge_pid` (34 lines, from 102). A refused sign-in is `Unauthenticated`; other failures are
`Unavailable` or `DeadlineExceeded`; the child is stopped on every failure. Both scopes now report:
the session-scoped `try_spawn_bridge` returns the outcome, and an unpreparable bridge fails
`FAILED_PRECONDITION` where it was skipped with a log line before. `DEFAULT_STREAM_WIDTH/HEIGHT`
(1920×1080) are removed: the reply carries the size the bridge saw, and `BridgeSpawnConfig` hints `0×0`.
`tddy-screenshare` became a normal dependency. Tests: `tests/bridge_readiness_acceptance.rs` (6) on a
stand-in bridge in `tests/common`; the inline `FakeBridge` prints a `ready` line.

Left open, as backlog entries: a session start that spawns no bridge still answers `0×0`
(`2026-10-20-session-start-without-a-bridge-succeeds`), the two stand-in bridges
(`2026-10-20-screen-sharing-two-stand-in-bridges`), `start_stream` at 72 lines
(`2026-10-20-screen-sharing-start-functions-over-length`), and the size fixed at start
(`2026-10-18-screen-share-mid-session-resize`). See [Screen-sharing service](../screen-sharing-service.md#a-start-waits-for-its-bridge).
