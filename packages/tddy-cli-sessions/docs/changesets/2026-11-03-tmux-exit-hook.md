# 2026-11-03 — A tmux-run main's exit hook runs on the agent's exit

**Type:** Feature

`start_agent_tmux_session(…, remain_on_exit)`; `tmux/pane.rs` (kept pane: `remain_on_exit_args`, `capture_final_screen`, `pane_state`); `cli_session_manager/tmux_exit_hook.rs` (`run_exit_hook_on_agent_exit`, `exit_hook_capture`; watchers closed by `kill_all`). Part of [#816](https://github.com/uppin/tddy-coder/pull/816) — see [module layout](../module-layout.md).
