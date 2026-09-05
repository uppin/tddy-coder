# 2026-07-01 — Sandboxed-action PTY session-end deadlock + ordering race

**Type:** Fix

`sandbox_action.rs`/`sandbox_plan_builder.rs`/`sandbox_session.rs`: session-end signaling now always defers `SessionEnded` to the next `HostPoll` (shared `tddy-sandbox-runner` fix) instead of pushing it immediately on the raw outbound stream, which could stall a not-yet-attached host indefinitely or race ahead of queued terminal output. Architecture [tddy-sandbox architecture](../../../../packages/tddy-sandbox/docs/architecture.md). (tddy-daemon, tddy-sandbox-runner)
