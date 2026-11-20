# 2026-11-20 — A session is live only while the daemon holds an agent tab

**Type:** Fix · **Stack:** `#session-liveness` 1/4 (PR #958)

The harness takes `with_held_sessions`: its list and branch-owner scan read the live set and `/delete` stops what the daemon holds. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
