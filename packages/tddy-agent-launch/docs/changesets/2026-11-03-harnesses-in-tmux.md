# 2026-11-03 — Every CLI harness can run in tmux

**Type:** Feature

`cli_harness_tmux.rs` (start, resume, lazy re-attach by recorded session type, `settle_unwatched_agent_exit`); cursor-cli start/resume in tmux as the session's OS user (`CursorProcess`, `relaunch_cursor`, `chat_to_resume`); the generic harness launch moved to `opencode_cli_spawn/harness_launch.rs`; `TMUX_RUN_SESSION_TYPES`. Part of [#816](https://github.com/uppin/tddy-coder/pull/816) — see [module layout](../module-layout.md).
