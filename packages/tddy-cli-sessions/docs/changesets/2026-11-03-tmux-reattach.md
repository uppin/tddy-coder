# 2026-11-03 — A tmux-run main is a client that can come and go

**Type:** Feature

`cli_session_manager/tmux_reattach.rs`: `classify_main_exit` / `MainExit`, `reattach_main`, `has_managed_workflow`, the detach watcher (`DetachWatch`, `DETACH_WATCH_INTERVAL`, `with_detach_watch_interval`). `kill_all` closes the watcher registry and detaches tmux-run mains instead of killing their tmux sessions (`TerminalEntry.os_user` removed). Only an agent's end — not a client's — drops the session's managed workflow and exposure. Part of [#815](https://github.com/uppin/tddy-coder/pull/815) — see [module-layout.md](../module-layout.md).
