# tddy-tui — Architecture

## Role

`tddy-tui` implements the ratatui view layer for `tddy-coder`: key events map to `UserIntent`, local `ViewState` tracks scroll and UI buffers, and `draw()` renders activity log, dynamic area, status bar, and prompt bar.

## Status bar

The status bar is a single `Paragraph` line. Text is built by `render::status_bar_text_for_draw`, which:

- Selects the spinner character from `SPINNER_FRAMES` using `ViewState::spinner_tick`.
- Resolves the visible session segment via `ui::first_hyphen_segment_of_workflow_session_id` from `PresenterState::workflow_session_id`.
- Uses `ui::format_status_bar_with_activity_prefix` when goal and state are present, or `ui::format_status_bar_idle` for the idle tail, then `ui::prepend_activity_to_status_line` to prefix spinner and segment.

`VirtualTui` calls the same `draw()` function so local and remote frames match.

## Module map (reference)

| Module | Responsibility |
|--------|----------------|
| `ui` | Elapsed formatting, status line strings, session segment rules, activity prefix |
| `render` | Frame layout, `draw`, question/inbox/plan/error sub-renderers |
| `view_state` | Spinner tick, scroll offsets, selection indices |
| `virtual_tui` | Headless terminal, input parsing, `apply_event` |
| `event_loop` | Crossterm loop, optional external intents and byte capture |

## Related packages

- **tddy-core**: `PresenterState`, `PresenterEvent`, workflow events
- **tddy-service / gRPC**: terminal byte streaming consumes the same render output as the local TUI

## Further reading

- [Feature: TUI status bar](../../../../docs/ft/coder/tui-status-bar.md)
- [Changesets](./changesets.md)
