# 2026-10-25 — A terminal stream can be resynced at any point

A terminal that drifted out of step with its session repairs itself without a reconnect. On a stream
opened with a snapshot, a client can ask for the current screen on any message
(`request_snapshot`); keystrokes on the same message are applied first, then the host sends a fresh
snapshot and continues live from it with contiguous offsets. Asking is a read, so a screen without
terminal control may ask too. A client the host's live output outran is sent a fresh snapshot
instead of a gap. Legacy `TAIL` / `FROM_OFFSET` streams are unchanged. See
[Terminal sessions § Resync at any point](../terminal-sessions.md#resync-at-any-point).
