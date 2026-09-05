# 2026-03-21 — Terminal: coder left the room

- **GhosttyTerminalLiveKit**: When the LiveKit **server/coder** participant disconnects (`ParticipantDisconnected` for `serverIdentity`), input to the RPC stream stops, the terminal is dimmed and non-interactive (`data-session-active="false"` on `GhosttyTerminal`), and a full-area banner **`terminal-coder-unavailable`** explains that the session ended.
