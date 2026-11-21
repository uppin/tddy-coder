# 2026-11-20 — Tabs are recorded in a roster, an agent tab can run in tmux, and live tmux tabs re-attach at startup

**Type:** Feature · **Stack:** `#session-liveness` 2/4 (PR #959)

`recorded_tabs.rs` (every tab recorded, closed tabs forgotten, a tmux-wrapped agent tab) and `reattach_live_tmux_tabs` (the startup re-attach of the main and every wrapped tab). Full record: [`docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
