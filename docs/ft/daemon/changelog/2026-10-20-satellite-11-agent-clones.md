# 2026-10-20 — The session-agent roster and its clones no longer name LiveKit

`#satellite` 11/13. Attaching an agent owned by another daemon, its hosted clone, the room admission
handshake and the roster broadcast all behave as they did before. The daemon's session-agent code
reaches the session room through the daemon's extension ports, and the LiveKit integration
implements them. See [Session agent roster](../session-agent-roster.md).
