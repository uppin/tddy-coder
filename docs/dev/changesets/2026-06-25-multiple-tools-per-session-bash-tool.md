# 2026-06-25 — Multiple tools per session (Bash tool)

**Type:** Feature

`connection.proto`: `StartTerminalSession`/`StopTerminalSession`/`ListTerminalSessions` (+ `TerminalSessionInfo{terminal_id,kind,pid}`), `terminal_id` on `SessionTerminalInput`/`StreamTerminalOutputRequest` (empty ⇒ `"main"`); `tddy-daemon`: two-level registry `session_id→(terminal_id→PtyHandle)`, `start_terminal`/`get_terminal`/`list_terminals`/`stop_terminal`, generalized `spawn_in_pty`, 3 terminal I/O handlers route by `terminal_id`, `"main"` stop guard, `session_deletion::signal_pid` exposed; `tddy-tools`: `pty_relay` literals carry `terminal_id`. Bash tool = `$SHELL` in worktree, no inputs. 12 acceptance tests. Feature [terminal-sessions.md](../ft/daemon/terminal-sessions.md); PR [#224](https://github.com/uppin/tddy-coder/pull/224). (tddy-service, tddy-daemon, tddy-tools)
