# 2026-06-25 — Multiple tools per session (Bash tool)

**Type:** Feature

`ClaudeCliSessionManager` two-level registry `session_id→(terminal_id→PtyHandle)`; `start_terminal` (Bash tool: `$SHELL` in worktree, kind `"bash"`, uuidv7 id, no inputs), `get_terminal`/`list_terminals`/`stop_terminal` (SIGTERM→SIGKILL, idempotent with PTY exit-monitor); `spawn_in_pty` generalized over a prebuilt argv (`spawn_tool` shared by claude + bash); `start_terminal_session`/`stop_terminal_session`/`list_terminal_sessions` handlers (auth, `"main"` stop guard → `INVALID_ARGUMENT`, worktree from main handle → `FAILED_PRECONDITION`, `$SHELL` resolve); 3 terminal I/O handlers route by `terminal_id` (empty ⇒ `"main"`, unknown ⇒ `NOT_FOUND`); `session_deletion::signal_pid` `pub(crate)`. 12 acceptance tests (`terminal_session_acceptance`). Feature [daemon/terminal-sessions.md](../../../../docs/ft/daemon/terminal-sessions.md). (tddy-daemon)
