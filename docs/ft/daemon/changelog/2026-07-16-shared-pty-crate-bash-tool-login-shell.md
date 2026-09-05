# 2026-07-16 — Shared PTY crate; Bash tool login shell

- The daemon's PTY runtime and registry moved into a shared `tddy-pty` crate (reused by tddy-coder for its bash terminal tabs); OS-user impersonation stays in the daemon over a thin adapter. See [terminal-sessions.md](../terminal-sessions.md).
- The Bash tool (`StartTerminalSession`) now spawns the target user's passwd login shell instead of the daemon's `$SHELL`.
