# tddy-tui — Architecture

## Role

`tddy-tui` implements the ratatui view layer for `tddy-coder`: key events map to `UserIntent`, local `ViewState` tracks scroll and UI buffers, and `draw()` renders activity log, dynamic area, status bar, and prompt bar.

## Status bar

The status bar is a single `Paragraph` line. Text is built by `render::status_bar_text_for_draw`, which:

- Resolves the activity character via `status_bar_activity::activity_prefix_char_for_draw` (spinner frames in agent-active `Running` mode; multi-phase ·/•/● heartbeat in user-question and other non-running modes that use idle treatment).
- Resolves displayed goal elapsed via `status_bar_activity::display_elapsed_for_goal_row` (live elapsed in agent-active mode; frozen snapshot in clarification waits when anchored).
- Resolves the visible session segment via `ui::first_hyphen_segment_of_workflow_session_id` from `PresenterState::workflow_session_id`.
- Uses `ui::format_status_bar_with_activity_prefix` when goal and state are present, or `ui::format_status_bar_idle` for the idle tail, then `ui::prepend_activity_to_status_line` to prefix activity and segment, then `render::inject_worktree_into_status_line` to weave in `PresenterState::active_worktree_display` before `Goal:` when set.

`ViewState` holds `spinner_tick` (advanced only in agent-active modes), frozen elapsed and idle animation anchors keyed to mode transitions, and related fields documented in `view_state.rs`.

`VirtualTui` calls the same `draw()` function so local and remote frames match. Its periodic re-render interval comes from `status_bar_activity::virtual_tui_periodic_render_interval` (short interval for agent-active animation, about one second in clarification waits). Cursor-only frame bytes are throttled against a minimum interval when CSI differencing indicates only cursor motion or visibility toggles.

## Markdown viewer (plan tail)

`AppMode::MarkdownViewer` uses a wrapped `Paragraph` for the activity area. Scroll limits derive from `Paragraph::line_count` with the same `Wrap { trim: true }` settings as render (via ratatui `unstable-rendered-line-info`). Approve/Reject labels append to the markdown `Text` only after the view marks end-of-markdown-scroll; they scroll with the document instead of a reserved footer strip.

## Prompt caret

`render::editing_prompt_cursor_position` returns a terminal cell when the active mode is typing in the prompt bar; `draw` calls `Frame::set_cursor_position`. The local `event_loop` runs crossterm `Show` after `draw` when that position is present.

## Module map (reference)

| Module | Responsibility |
|--------|----------------|
| `status_bar_activity` | Agent-active vs idle rules, displayed elapsed, activity glyph, VirtualTui periodic interval |
| `ui` | Elapsed formatting, status line strings, session segment rules, activity prefix |
| `render` | Frame layout, `draw`, question/inbox/plan/error sub-renderers |
| `view_state` | Spinner tick, scroll offsets, selection indices, frozen status anchors |
| `virtual_tui` | Headless terminal, input parsing, `apply_event`, frame diff + cursor-only throttle |
| `event_loop` | Crossterm loop, optional external intents and byte capture, caret `Show` when editing |

## Related packages

- **tddy-core**: `PresenterState`, `PresenterEvent`, workflow events, `AppMode`
- **tddy-service / gRPC**: terminal byte streaming consumes the same render output as the local TUI

## Further reading

- [Feature: TUI status bar](../../../../docs/ft/coder/tui-status-bar.md)
- [Changesets](./changesets.md)
