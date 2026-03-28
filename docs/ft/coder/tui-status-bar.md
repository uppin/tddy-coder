# TUI status bar — activity and session segment

**Status:** Stable  
**Product area:** Coder (TUI)

## Summary

The interactive and virtual TUI status line shows a single cohesive row: a cycling spinner frame, a short workflow session identifier, then the existing goal/state/elapsed/agent content. Remote terminal streams (`VirtualTui`, gRPC `StreamTerminal`) use the same `draw()` path as the local TUI, so clients see identical layout.

## Layout

Left to right on the status bar:

1. **Spinner** — One character from a fixed frame sequence (`|`, `/`, `-`, `\`), advanced once per render when the terminal is large enough; driven by `ViewState::spinner_tick` with the same cadence as the historical corner overlay.
2. **Session segment** — The first 8-character hexadecimal field when the engine session id matches UUID-shaped input (first hyphen-separated field). When the id is missing, empty, or not UUID-shaped at that position, a fixed em-dash placeholder (`U+2014`) appears so the column width stays predictable.
3. **Goal:** … **State:** … **elapsed** … **agent** **model** … scroll hint — Unchanged semantics and order after `Goal:`.

## Presenter state

`PresenterState::workflow_session_id` holds the optional engine session string. It is set when `ProgressEvent::SessionStarted` arrives and when `start_workflow` receives an initial session id; it is cleared on workflow completion (success or error) and before an inbox-driven workflow restart, until a new `SessionStarted` supplies a value.

## Operational notes

- Status-bar formatting helpers live in `packages/tddy-tui/src/ui.rs`; frame composition for the status row lives in `packages/tddy-tui/src/render.rs`.
- Per-frame diagnostic detail uses `log::trace!` on the hot path; presenter lifecycle messages truncate long ids in debug logs.
- Direct `println!` / `eprintln!` are not used on TUI draw paths (ratatui display integrity).

## Related documentation

- [gRPC remote control](grpc-remote-control.md) — terminal streaming and Virtual TUI
- [Coder overview](1-OVERVIEW.md) — TUI capability summary
