# 2026-11-03 — A session terminal refuses an unreachable OS user up front

A daemon that cannot become a session's OS user now refuses its terminals with the same "cannot become user" message as the tmux listing, before any terminal opens, instead of starting one that fails inside. See [Terminal sessions](../terminal-sessions.md).
