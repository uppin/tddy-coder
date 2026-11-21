# 2026-11-20 — A session is live only while the daemon holds an agent tab

**Type:** Fix · **Stack:** `#session-liveness` 1/4 (PR #958)

`LaunchSessions::live_sessions()` feeds listing, ensure-home and the idle reaper; `signal_held_session` signals the held main and refuses with `session has no running agent`; delete stops the session's terminals; `reattach_recorded_main` returns a `ReattachOutcome`. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
