# 2026-06-25 — **Multiple tools per session

**Type:** Feature

connection.proto** — `ConnectionService` gains `StartTerminalSession`/`StopTerminalSession`/`ListTerminalSessions` (+ `StartTerminalSessionRequest/Response`, `StopTerminalSessionRequest/Response`, `ListTerminalSessionsRequest/Response`, `TerminalSessionInfo{terminal_id,kind,pid}`); `terminal_id` field on `SessionTerminalInput` (4) and `StreamTerminalOutputRequest` (3), empty ⇒ the reserved `"main"` terminal. Feature [daemon/terminal-sessions.md](../../../../docs/ft/daemon/terminal-sessions.md). (tddy-service)
