# 2026-11-02 — Attach a running tmux session as a tab

The **+** menu of a session's tab strip lists the tmux sessions running on the session's host for the
session's OS user — the ones started in that user's own terminals — with their window count and
attached clients. Choosing one opens a **tmux: <name>** tab attached to it, shared with everyone
viewing the session. Closing the tab detaches: the tmux session keeps running and can be attached
again from tddy or any terminal. The menu says **No tmux sessions** when there are none and shows the
reason when tmux is not available. tddy asks tmux to forward its copy-mode copies (OSC 52) for that
client only and never changes the user's tmux configuration. Tool (tddy-coder) sessions offer no tmux.
See [Session terminal tabs](../session-terminal-tabs.md) and
[Terminal sessions § tmux terminals](../../daemon/terminal-sessions.md#tmux-terminals).
