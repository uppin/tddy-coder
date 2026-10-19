# 2026-10-19 — A session's terminal is exposed through a port

`#satellite` 6/13. A claude-cli session's terminal is still served to remote participants over LiveKit, after a
remote reach for most sessions and at once for a Telegram-started one. It now goes through a terminal-exposure
port; a daemon with no LiveKit credentials exposes nothing, and the terminal stays reachable over the daemon's
own connection. See [extension ports](../../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-terminal-exposure-port-terminal_exposure).
