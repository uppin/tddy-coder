# TUI status bar — activity and session segment

**Status:** Stable  
**Product area:** Coder (TUI)

## Summary

The interactive and virtual TUI status line shows a single cohesive row: an activity indicator, a short workflow session identifier, then goal/state/elapsed/agent content. Remote terminal streams (`VirtualTui`, gRPC `StreamTerminal`) use the same `draw()` path as the local TUI, so clients see identical layout and timing semantics.

## Layout

Left to right on the status bar:

1. **Activity indicator** — Behavior depends on workflow mode:
   - **Agent-active** (`AppMode::Running`): One character from the spinner frame sequence (`|`, `/`, `-`, `\`), driven by `ViewState::spinner_tick` at the historical fast cadence (four ticks per frame advance).
   - **User-question wait** (`Select`, `MultiSelect`, `TextInput`): A single-column pulse using middle dot U+00B7 (`·`) and bullet U+2022 (`•`) on a one-second phase, alternating small and large appearance. The fast spinner sequence does not run in these modes.
   - **Other modes** (for example `FeatureInput`, `DocumentReview`, `MarkdownViewer`, `ErrorRecovery`): When a goal row is shown, the same idle pulse and frozen elapsed rules apply as for user-question waits unless product classification treats the mode as agent-active in a future revision.
2. **Session segment** — The first 8-character hexadecimal field when the engine session id matches UUID-shaped input (first hyphen-separated field). When the id is missing, empty, or not UUID-shaped at that position, a fixed em-dash placeholder (`U+2014`) keeps the column width predictable.
3. **Goal:** … **State:** … **elapsed** … **agent** **model** … scroll hint — Segment order after `Goal:` matches the historical layout.

## Elapsed time on the goal row

- **Agent-active**: The numeric elapsed segment reflects live duration from `PresenterState::goal_start_time`.
- **User-question wait** (`Select`, `MultiSelect`, `TextInput`): The displayed elapsed value is frozen at the value captured when entering the mode (or the equivalent anchor), so wall time does not advance the clock while the user stays in that mode.

## Presenter state

`PresenterState::workflow_session_id` holds the optional engine session string. It is set when `ProgressEvent::SessionStarted` arrives and when `start_workflow` receives an initial session id; it is cleared on workflow completion (success or error) and before an inbox-driven workflow restart, until a new `SessionStarted` supplies a value.

## Virtual TUI and streaming

- **Periodic render**: Headless `VirtualTui` wakes on a short interval for agent-active modes so the spinner animates smoothly; in clarification waits the interval is about one second so the idle dot pulse and frozen clock align with visible updates without high-frequency full-frame traffic when only the idle phase advances.
- **Frame diffing**: Bytes go to the client when the rendered frame differs from the previous frame (existing behavior).

## Operational notes

- Status-bar policy helpers live in `packages/tddy-tui/src/status_bar_activity.rs`; formatting helpers in `packages/tddy-tui/src/ui.rs`; frame composition in `packages/tddy-tui/src/render.rs`.
- Per-frame diagnostic detail uses `log::trace!` on the hot path; presenter lifecycle messages truncate long ids in debug logs.
- Direct `println!` / `eprintln!` are not used on TUI draw paths (ratatui display integrity).

## Related documentation

- [gRPC remote control](grpc-remote-control.md) — terminal streaming and Virtual TUI
- [Coder overview](1-OVERVIEW.md) — TUI capability summary
