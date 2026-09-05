# 2026-06-26 — Single-screen terminal control mutex

**Type:** Feature

`connection.proto`: `ClaimTerminalControl`/`WatchTerminalControl` RPCs, `control_token` on `SessionTerminalInput`/`SignalSessionRequest`/`Start/StopTerminalSessionRequest`; `tddy-daemon`: `ControlLeaseInfo`/`ControlChangeEvent`/`ControlRegistry` in `ClaudeCliSessionManager` (`claim_control`/`verify_control`/`subscribe_control`), `relay_control_events` helper, `FailedPrecondition` gate in `send_terminal_input`+`stream_session_terminal_io`; `tddy-web`: `screenId.ts`, `terminalControlState.ts` reducer, `useTerminalControl` hook (`runControlSession` helper), `SessionMainPane` scrim overlay + "Claim terminal" button. Tests: 10 Rust acceptance, 4 Cypress CT, 9 bun unit. Feature [terminal-sessions.md](../ft/daemon/terminal-sessions.md) + [session-drawer.md](../ft/web/session-drawer.md); PR [#228](https://github.com/uppin/tddy-coder/pull/228). (tddy-service, tddy-daemon, tddy-tools, tddy-web)
