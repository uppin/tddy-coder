# 2026-11-02 — `ListTmuxSessions` and the tmux terminal kind

**Type:** Feature

`ListTmuxSessions` answers through the new `TmuxHost` port (`TerminalSessionPorts.tmux`; `NoTmux`
refuses with `FAILED_PRECONDITION`). `TerminalKind::Tmux { session }` is what
`TerminalKind::from_request(kind, tmux_session)` reads for kind `"tmux"` (`TMUX_KIND`); a tmux kind
without a session is `INVALID_ARGUMENT`. `TerminalLaunch.os_user` carries the token's OS user to the
roster, and `TerminalDescriptor.tmux_session` reaches `TerminalSessionInfo.tmux_session`. Part of
[#806](https://github.com/uppin/tddy-coder/pull/806) — see
[terminal-session-service.md](../terminal-session-service.md). `service.rs` 522 → 546 production lines
(`oversized-file-service`, not split: stack-shared).
