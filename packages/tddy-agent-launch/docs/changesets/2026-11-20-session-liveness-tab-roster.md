# 2026-11-20 — Tabs are recorded in a roster, an agent tab can run in tmux, and live tmux tabs re-attach at startup

**Type:** Feature · **Stack:** `#session-liveness` 2/4 (PR #959)

`tmux_main_roster` records the main's tmux run in the roster and `reattach_recorded_main` reads it, falling back to `.session.yaml`; `attach_and_record_main` in `start_harness_in_tmux`. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
