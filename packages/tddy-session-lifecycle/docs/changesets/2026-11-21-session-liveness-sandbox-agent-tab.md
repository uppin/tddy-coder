# 2026-11-21 — A sandboxed session's agent tab runs in a jail of its own

**Type:** Feature · **Stack:** `#session-liveness` 7/8 (PR #966)

The roster starts a jailed agent tab (`start_jailed_agent_tab`), records `[sandbox, agent]`, lists it as its agent, and restores it on Resume.

Full record: [`docs/dev/changesets/2026-11-21-session-liveness-sandbox-agent-tab.md`](../../../../docs/dev/changesets/2026-11-21-session-liveness-sandbox-agent-tab.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#a-sandboxed-sessions-tabs).
