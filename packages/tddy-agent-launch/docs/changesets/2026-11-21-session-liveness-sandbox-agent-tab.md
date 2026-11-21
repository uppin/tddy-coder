# 2026-11-21 — A sandboxed session's agent tab runs in a jail of its own

**Type:** Feature · **Stack:** `#session-liveness` 7/8 (PR #966)

`TabJailAgentPlanner` and `TabJailSpawn::spawn_agent` for `LaunchSessions` (the agent's grants and exec plan under the tab's own paths, no session record written), `TabRestore::JailedAgent`, and the jailed agent tabs in `live_sessions`.

Full record: [`docs/dev/changesets/2026-11-21-session-liveness-sandbox-agent-tab.md`](../../../../docs/dev/changesets/2026-11-21-session-liveness-sandbox-agent-tab.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#a-sandboxed-sessions-tabs).
