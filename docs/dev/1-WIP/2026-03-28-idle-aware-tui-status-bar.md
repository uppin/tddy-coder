# Changeset: Idle-aware TUI status bar and VirtualTui cadence

**Date**: 2026-03-28  
**Status**: Complete  
**Type**: Feature

## Affected Packages

- `tddy-tui`
- `tddy-e2e` (gRPC terminal and reconnect tests)

## Related feature documentation

- [docs/ft/coder/tui-status-bar.md](../../ft/coder/tui-status-bar.md)

## Summary

The status bar distinguishes agent work from user-input gates: agent-active modes show a fast spinner and live goal elapsed; clarification waits show a frozen elapsed display and a one-second ·/• pulse. Virtual TUI periodic rendering follows the same cadence expectations.

## Technical changes (State B)

### tddy-tui

- `status_bar_activity`: `status_activity_is_agent_active`, `display_elapsed_for_goal_row`, `activity_prefix_char_for_draw`, `virtual_tui_periodic_render_interval`.
- `ViewState`: frozen goal elapsed and idle animation anchors on mode transitions.
- `render::status_bar_text_for_draw` and `VirtualTui`: shared `draw()` path; interval selection for autonomous renders.

### tddy-e2e

- gRPC terminal tests assert idle animation cadence and frozen-clock behavior; reconnect acceptance uses a byte threshold compatible with smaller idle status frames.

## Documentation applied

- `docs/ft/coder/tui-status-bar.md`, `docs/ft/coder/grpc-remote-control.md`, `docs/ft/coder/changelog.md`
- `packages/tddy-tui/docs/architecture.md`, `packages/tddy-tui/docs/changesets.md`
