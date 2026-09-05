# 2026-07-16 — Session terminal tabs

- The session detail pane now has a terminal tab bar: an **Agent** tab (the coding agent, not closable) plus one closable tab per interactive **bash** terminal, with a `+` to open more; switching tabs keeps every terminal of the session mounted and streaming in the background.
- Works for local (gRPC) and remote/coder (LiveKit) sessions, reusing the existing `terminal_id`-addressed `ConnectionService` terminal RPCs — no protocol changes. See [session-terminal-tabs.md](../session-terminal-tabs.md).
