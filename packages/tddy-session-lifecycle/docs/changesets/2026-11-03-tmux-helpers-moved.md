# 2026-11-03 — tmux helpers moved to `tddy-cli-sessions`

**Type:** Refactor

`svc_terminal_ports/tmux_host.rs` keeps only `HostTmux`; the roster imports the tab helpers from `tddy_cli_sessions::tmux`. No behaviour change for tmux tabs. Acceptance: `claude_cli_tmux_session_acceptance` (tmux-run sessions, incl. shutdown) and a `tmux_tabs_acceptance` case pinning that shutdown leaves a tab's tmux session running. Part of [#814](https://github.com/uppin/tddy-coder/pull/814) — see [module-layout.md](../module-layout.md).
