# 2026-11-20 — Tabs are recorded in a roster, an agent tab can run in tmux, and live tmux tabs re-attach at startup

**Type:** Feature · **Stack:** `#session-liveness` 2/4 (PR #959)

`TabRosterStore` (the one roster writer), `start_terminal_under_id`, `attach_terminal_to_tmux` / `reattach_terminal`, per-terminal detach watchers and settling holds, `agent_tab_tmux_session_name`. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
