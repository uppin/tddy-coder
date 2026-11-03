# 2026-11-03 — Initial terminal content copies once; tmux menu says when the host is not connected

A terminal's initial content is rendered once, so an OSC 52 copy inside it reaches the clipboard once. The "+" menu's tmux section shows "host not connected" instead of "Loading tmux sessions…" for ever while the session's host has not connected. See [Session terminal tabs](../session-terminal-tabs.md) and [Web terminal](../web-terminal.md).
