# 2026-07-01 — Shared runner: `SessionEnded` session-end signaling always deferred to `HostPoll`

**Type:** Fix

documented + fixed a deadlock/ordering-race pair in `tddy-sandbox-runner`: an immediate push of `SessionEnded` on the raw outbound stream could stall a not-yet-attached host forever, or race ahead of queued `terminal_backlog` output; delivery now always waits for the next `HostPoll` reply, after backlog drain. Architecture [architecture.md](../architecture.md). (tddy-sandbox, tddy-sandbox-runner, tddy-daemon, tddy-service)
