# 2026-11-03 — `tmux` module, the main terminal as a tmux client

**Type:** Feature

`tddy_cli_sessions::tmux` holds the host's tmux helpers (moved from `tddy-session-lifecycle`) and the agent's tmux session operations (`agent_tmux_session_name`, `start_agent_tmux_session`, `kill_tmux_session`, `tmux_session_alive`, `pane_pid`). `attach_main_to_tmux` runs the session's main terminal as a tmux client. `TerminalEntry` records the OS user; `kill_all` kills main terminals' tmux sessions as that user (a tab's tmux session is left running). `with_claude_launch_flags` is the PTY and tmux launches' shared argv step. Part of [#814](https://github.com/uppin/tddy-coder/pull/814) — see [module-layout.md](../module-layout.md#tmux-terminals).
