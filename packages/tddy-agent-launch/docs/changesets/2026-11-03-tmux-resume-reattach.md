# 2026-11-03 — Resume re-attaches a live tmux-run agent

**Type:** Feature

`claude_cli_tmux`: `reattach_claude_in_tmux`, `agent_tmux_session_is_alive`, `reattach_recorded_main` (the lazy re-attach, registering the main recording); `relaunch_claude_cli` re-attaches when the tmux session lives, else starts fresh. `resume_claude_cli_session` keeps a managed workflow this daemon still serves for a live tmux-run agent (`resumed_recipe`); unchanged in length (142). Part of [#815](https://github.com/uppin/tddy-coder/pull/815) — see [module-layout.md](../module-layout.md).
