# 2026-11-20 — A session is live only while the daemon holds an agent tab

**Type:** Fix · **Stack:** `#session-liveness` 1/4 (PR #958)

The host exposes the live set (`DaemonTerminalSessionStore::live_sessions`, `signal_session_dir`); `TestDaemon::holds_a_main_agent` makes a session live in acceptance tests; `session_liveness_acceptance`. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
