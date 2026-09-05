# 2026-06-25 — Multiple tools per session (Bash tool)

- A session can run multiple identified tools, not just `claude`: the main terminal is the reserved id `"main"` (kind `"claude-cli"`); on-demand **Bash** tools (kind `"bash"`) run `$SHELL` (fallback `/bin/bash`) in the worktree, no inputs
- New `ConnectionService` RPCs `StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` (`TerminalSessionInfo{terminal_id, kind, pid}`); stopping `"main"` is rejected with `INVALID_ARGUMENT`
- Terminal I/O RPCs (`StreamSessionTerminalIO`, `StreamTerminalOutput`, `SendTerminalInput`) gain an optional `terminal_id` (empty ⇒ `"main"`); unknown id → `NOT_FOUND`
- RPC-only; no web UI integration in this release
