# 2026-06-26 — Single-screen terminal control mutex

- Per-session exclusive control lease in `ClaudeCliSessionManager`: first browser tab to attach becomes the controller; subsequent tabs see a **"Claim terminal"** overlay and cannot send input
- New `ConnectionService` RPCs: `ClaimTerminalControl` (unary, `steal` flag to evict the current holder) and `WatchTerminalControl` (server-stream, snapshot-then-delta via broadcast channel)
- `control_token` field added to `SessionTerminalInput`, `SignalSessionRequest`, `StartTerminalSessionRequest`, `StopTerminalSessionRequest`; input RPCs return `FAILED_PRECONDITION` when the token is wrong
- Uncontrolled sessions (no lease held) accept all inputs — fully backwards compatible
