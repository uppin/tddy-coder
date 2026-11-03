# 2026-11-03 — `TmuxMainReattach`

**Type:** Feature

The daemon's `MainReattach` port (`svc_terminal_ports.rs`): re-attaches a tmux-run session's `main` terminal on first access through `tddy_agent_launch::claude_cli_tmux::reattach_recorded_main`, single-flight per session. Acceptance: `tests/claude_cli_tmux_reattach_acceptance.rs` (8 scenarios: shutdown, restart and first stream, live and gone resume, client exit, same-daemon resume, the detach watcher, agent exit); n1's shutdown test in `claude_cli_tmux_session_acceptance.rs` now asserts the detach. Part of [#815](https://github.com/uppin/tddy-coder/pull/815) — see [module-layout.md](../module-layout.md).
