# 2026-11-23 — A target session's tmux tabs run on its target

- The **+** menu of a Coder/SSH target session lists the target's tmux sessions, not the daemon host's.
- Attaching to one, **New tmux session…** and **<agent> in tmux** run in tmux on the target, in the session's worktree there; closing an agent-in-tmux tab kills its tmux session on the target.
- Resume re-attaches these tabs, or makes their tmux session again when it is gone (reported fresh). After a daemon restart they come back on the next Resume.

Feature: [terminal-sessions.md § tmux terminals](../terminal-sessions.md#tmux-terminals) · [session-terminal-tabs.md](../../web/session-terminal-tabs.md)
