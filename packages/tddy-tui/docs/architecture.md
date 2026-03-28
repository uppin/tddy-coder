# tddy-tui — Architecture

## Role

`tddy-tui` implements the ratatui view layer for `tddy-coder`: key events map to `UserIntent`, local `ViewState` tracks scroll and UI buffers, and `draw()` renders activity log, dynamic area, status bar, and prompt bar.

## Status bar

The status bar is a single `Paragraph` line. Text is built by `render::status_bar_text_for_draw`, which:

- Resolves the activity character via `status_bar_activity::activity_prefix_char_for_draw` (spinner frames in agent-active `Running` mode; one-second ·/• pulse in user-question and other non-running modes that use idle treatment).
- Resolves displayed goal elapsed via `status_bar_activity::display_elapsed_for_goal_row` (live elapsed in agent-active mode; frozen snapshot in clarification waits when anchored).
- Resolves the visible session segment via `ui::first_hyphen_segment_of_workflow_session_id` from `PresenterState::workflow_session_id`.
- Uses `ui::format_status_bar_with_activity_prefix` when goal and state are present, or `ui::format_status_bar_idle` for the idle tail, then `ui::prepend_activity_to_status_line` to prefix activity and segment.

`ViewState` holds `spinner_tick` (advanced only in agent-active modes), frozen elapsed and idle animation anchors keyed to mode transitions, and related fields documented in `view_state.rs`.

`VirtualTui` calls the same `draw()` function so local and remote frames match. Its periodic re-render interval comes from `status_bar_activity::virtual_tui_periodic_render_interval` (short interval for agent-active animation, about one second in clarification waits).

## Module map (reference)

| Module | Responsibility |
|--------|----------------|
| `status_bar_activity` | Agent-active vs idle rules, displayed elapsed, activity glyph, VirtualTui periodic interval |
| `ui` | Elapsed formatting, status line strings, session segment rules, activity prefix |
| `render` | Frame layout, `draw`, question/inbox/plan/error sub-renderers |
| `view_state` | Spinner tick, scroll offsets, selection indices, frozen status anchors |
| `virtual_tui` | Headless terminal, input parsing, `apply_event` |
| `event_loop` | Crossterm loop, optional external intents and byte capture |

## Related packages

- **tddy-core**: `PresenterState`, `PresenterEvent`, workflow events, `AppMode`
- **tddy-service / gRPC**: terminal byte streaming consumes the same render output as the local TUI

## Further reading

- [Feature: TUI status bar](../../../../docs/ft/coder/tui-status-bar.md)
- [Changesets](./changesets.md)
