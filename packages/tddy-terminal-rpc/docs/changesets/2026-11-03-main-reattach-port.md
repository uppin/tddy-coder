# 2026-11-03 — The `MainReattach` port

**Type:** Feature

`TerminalSessionPorts.main_reattach: Arc<dyn MainReattach>` (`NoMainReattach` for hosts that re-attach nothing). `ensure_main` consults it from `StreamSessionTerminalIo`, `StreamTerminalOutput`, `SendTerminalInput` and `GetTerminalHistory` for terminal `main` only, when the store has no running one, with the token's OS user. `service.rs` 546 → 573 production lines (record updated). Part of [#815](https://github.com/uppin/tddy-coder/pull/815) — see [terminal-session-service.md](../terminal-session-service.md#the-four-ports).
