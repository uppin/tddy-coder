# 2026-03-28 — TUI status bar: spinner and session segment

- **Status line**: The activity spinner lives in the status bar before `Goal:` (not a separate top-right cell). A short segment derived from the workflow engine session id appears between the spinner and `Goal:` when the id matches UUID first-field rules; otherwise a fixed em-dash placeholder keeps the column stable.
- **Presenter**: `PresenterState::workflow_session_id` reflects `SessionStarted` and `start_workflow`; it clears on workflow completion and before inbox-driven restarts.
- **Remote parity**: Virtual TUI and gRPC terminal streams use the same `draw()` formatting as the local TUI.
- **Docs**: [tui-status-bar.md](../tui-status-bar.md), `packages/tddy-tui/docs/architecture.md`.
