# 2026-11-02 — `start_tmux_terminal`

**Type:** Feature

`CliSessionManager::start_tmux_terminal` spawns a tmux client as the session's OS user
(`spawn_tool` with `os_user: Some(_)`), kind `"tmux"`, and records the tmux session on
`PtyHandle::tmux_session` and on the terminal's index entry. Part of
[#806](https://github.com/uppin/tddy-coder/pull/806) — see [module-layout.md](../module-layout.md#tmux-terminals).
