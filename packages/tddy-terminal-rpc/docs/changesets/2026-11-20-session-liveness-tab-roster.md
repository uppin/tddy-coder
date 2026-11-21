# 2026-11-20 — Tabs are recorded in a roster, an agent tab can run in tmux, and live tmux tabs re-attach at startup

**Type:** Feature · **Stack:** `#session-liveness` 2/4 (PR #959)

`LaunchNode`, `wrappers` and `launch` on the proto, `TerminalKind::TmuxWrappedAgent` and `from_wrapped_request`, `TerminalRoster::launch_chains`; `service.rs` split into `service/entry.rs` and `service/control_relay.rs` (577 → 467 production lines). Full record: [`docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-tab-roster.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
