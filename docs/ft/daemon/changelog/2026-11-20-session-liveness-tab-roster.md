# 2026-11-20 — Tabs are recorded, an agent tab can run in tmux, and live tmux agents re-attach at startup

Every terminal tab a session opens is recorded in the session's tab roster with the chain it was launched
through, and closing a tab removes it. An agent tab can run wrapped in tmux (`StartTerminalSession` with
`wrappers: [tmux]`), in a tmux session of its own as the session's OS user, so it survives its client and
a daemon restart; closing the tab ends it. At startup the daemon re-attaches every tmux main and tmux-wrapped
tab whose tmux session is alive, so such a session lists active without anyone opening it. See
[session-liveness.md](../session-liveness.md) and [terminal-sessions.md](../terminal-sessions.md).
