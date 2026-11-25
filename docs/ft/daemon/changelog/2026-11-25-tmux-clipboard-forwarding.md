# 2026-11-25 — Copies from programs in tmux-run sessions reach the viewer's clipboard

- A session run in tmux (`run_in_tmux`, on the host or on a Coder/SSH target) forwards programs'
  OSC 52 clipboard writes to the viewer: the agent session's window passes wrapped writes through,
  and the tmux server's `set-clipboard` is upgraded from the default `external` (which swallows
  plain writes) to `on`. An explicitly disabled server clipboard is left alone, and
  `~/.tmux.conf` is never touched.
- The target tmux commands announce the clipboard feature, as host tabs do; agent tabs enable
  forwarding on their session before they attach. A person's own tmux sessions keep their
  configuration untouched.

Feature: [terminal-sessions.md § tmux terminals](../terminal-sessions.md#tmux-terminals)
