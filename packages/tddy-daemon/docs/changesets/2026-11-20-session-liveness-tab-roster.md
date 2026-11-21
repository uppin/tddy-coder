# 2026-11-20 — Tabs are recorded in a roster, an agent tab can run in tmux, and live tmux tabs re-attach at startup

**Type:** Feature · **Stack:** `#session-liveness` 2/4 (PR #959)

the startup task that calls `reattach_live_tmux_tabs` once the host has bound the agent-tool sockets; `RuntimeTasks::spawn` is one function per task. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
