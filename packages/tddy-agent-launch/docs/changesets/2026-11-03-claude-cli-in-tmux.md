# 2026-11-03 — A claude-cli agent in tmux

**Type:** Feature

`claude_cli_tmux`: start (`start_claude_in_tmux`), resume (`resume_claude_in_tmux`, `relaunch_claude_cli`); `spawn_claude_cli_process` picks tmux or the PTY child. `refuse_a_conflicting_tmux_run_start` in `start_session_core_cancellable`. `DeleteSession` kills the tmux session first (`kill_recorded_tmux_session`). `resume_claude_cli_session` 142 → 141 lines. Part of [#814](https://github.com/uppin/tddy-coder/pull/814) — see [module-layout.md](../module-layout.md).
