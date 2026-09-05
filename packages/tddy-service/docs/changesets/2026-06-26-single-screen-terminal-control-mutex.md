# 2026-06-26 — **Single-screen terminal control mutex

**Type:** Feature

connection.proto** — `ConnectionService` gains `ClaimTerminalControl` (unary: `ClaimTerminalControlRequest{session_token,session_id,screen_id,steal}`, `ClaimTerminalControlResponse{granted,control_token,current_holder_screen_id}`) and `WatchTerminalControl` (server-stream: `WatchTerminalControlRequest`, `TerminalControlEvent{holder_screen_id,you_are_controller}`); `control_token` field added to `SessionTerminalInput` (5), `SignalSessionRequest` (4), `StartTerminalSessionRequest` (3), `StopTerminalSessionRequest` (4). TypeScript codegen regenerated. Feature [daemon/terminal-sessions.md](../../../../docs/ft/daemon/terminal-sessions.md). (tddy-service)
