# 2026-11-20 — A session is live only while the daemon holds an agent tab

**Type:** Fix · **Stack:** `#session-liveness` 1/4 (PR #958)

`list_sessions_in_dir(sessions_base, &LiveSessions)` marks a session active from the live set alone; `DaemonSessionListing::new(live)`; delete signals a recorded pid only for a `tool` session; the pack resolver reads `PackExecutionContext::live_sessions`. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
