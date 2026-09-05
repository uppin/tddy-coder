# 2026-06-26 — Single-screen terminal control mutex

**Type:** Feature

`ClaudeCliSessionManager`: `ControlLeaseInfo`/`ControlChangeEvent` types, `control: RwLock<HashMap<session_id, ControlLeaseInfo>>` + `control_tx: broadcast::Sender`, `claim_control`/`verify_control`/`current_control`/`subscribe_control`; `relay_control_events` free fn (extracted to reduce nesting); `claim_terminal_control`/`watch_terminal_control` handlers (auth, snapshot-then-delta, `ClaimOutcome` → proto response); `send_terminal_input` + `stream_session_terminal_io` enforcement gate (FAILED_PRECONDITION on wrong token). 10 acceptance tests. Feature [daemon/terminal-sessions.md](../../../../docs/ft/daemon/terminal-sessions.md). (tddy-daemon)
