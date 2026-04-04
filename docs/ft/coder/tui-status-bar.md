# TUI status bar — activity, session segment, and worktree

**Status:** Stable  
**Product area:** Coder (TUI)

## Summary

The interactive and virtual TUI status line shows a single cohesive row: an activity indicator, a short workflow session identifier, an optional worktree display segment when the presenter has one, then goal/state/elapsed/agent content. Remote terminal streams (`VirtualTui`, gRPC `StreamTerminal`) use the same `draw()` path as the local TUI, so clients see identical layout and timing semantics.

## Layout

Left to right on the status bar:

1. **Activity indicator** — Behavior depends on workflow mode:
   - **Agent-active** (`AppMode::Running`): One character from the spinner frame sequence (`|`, `/`, `-`, `\`), driven by `ViewState::spinner_tick` at the historical fast cadence (four ticks per frame advance).
   - **User-question wait** (`Select`, `MultiSelect`, `TextInput`) and other non-running modes that use idle treatment: A repeating heartbeat on the leading glyph: middle dot U+00B7 (`·`), bullet U+2022 (`•`), black circle U+25CF (`●`), then bullet again, on a short sub-second phase cycle so the pulse reads as small→large→small over a few seconds on an 80×24 terminal. The fast spinner sequence does not run in these modes.
   - **Other modes** (for example `FeatureInput`, `DocumentReview`, `MarkdownViewer`, `ErrorRecovery`): When a goal row is shown, the same idle heartbeat and frozen elapsed rules apply as for user-question waits unless product classification treats the mode as agent-active in a future revision.
2. **Session segment** — The first 8-character hexadecimal field when the engine session id matches UUID-shaped input (first hyphen-separated field). When the id is missing, empty, or not UUID-shaped at that position, a fixed em-dash placeholder (`U+2014`) keeps the column width predictable.
3. **Worktree segment** (optional) — When `PresenterState::active_worktree_display` carries a non-empty string (derived from the filesystem path at `WorkflowEvent::WorktreeSwitched`), the status line includes that label before the `Goal:` token, separated by `│`, without altering spinner or session order.
4. **Goal:** … **State:** … **elapsed** … **agent** **model** … scroll hint — Segment order after `Goal:` matches the historical layout.

## Elapsed time on the goal row

- **Agent-active**: The numeric elapsed segment reflects live duration from `PresenterState::goal_start_time`.
- **User-question wait** (`Select`, `MultiSelect`, `TextInput`): The displayed elapsed value is frozen at the value captured when entering the mode (or the equivalent anchor), so wall time does not advance the clock while the user stays in that mode.

## Presenter state

`PresenterState::workflow_session_id` holds the optional engine session string. It is set when `ProgressEvent::SessionStarted` arrives and when `start_workflow` receives an initial session id; it is cleared on workflow completion (success or error) and before an inbox-driven workflow restart, until a new `SessionStarted` supplies a value.

`PresenterState::active_worktree_display` holds an optional short label for the active git worktree directory (basename with length cap). It is set when `WorkflowEvent::WorktreeSwitched` fires, using `presenter::worktree_display::format_worktree_for_status_bar`. The field stays empty until that event supplies a path the formatter turns into a non-empty display string.

## Prompt caret (local TUI)

Text-editing prompt modes (`FeatureInput`, running follow-up input, `TextInput`, Select/MultiSelect “other” typing, plan refinement while `plan_refinement_pending`) position the hardware cursor after `draw()` at the UTF-8-safe insert index for the char-wrapped prompt `Paragraph`. The local event loop issues crossterm `Show` when a caret position applies so the insert point remains visible.

## Plan markdown viewer (Approve / Reject)

In `MarkdownViewer`, **Approve** and **Reject** do not occupy a fixed footer band before the user reaches the end of the markdown scroll range. After the scroll position reaches the bottom of the markdown body, both actions appear as trailing lines inside the same scrollable `Paragraph` as the plan content (wrapped with the same rules as the body). The prompt bar text lists Alt+A / Alt+R only when the viewer reports end-of-scroll (`markdown_at_end`).

## Virtual TUI and streaming

- **Periodic render**: Headless `VirtualTui` wakes on a short interval for agent-active modes so the spinner animates smoothly; in clarification waits the interval is about one second so the idle heartbeat and frozen clock align with visible updates without high-frequency full-frame traffic when only the idle phase advances.
- **Frame diffing**: Bytes go to the client when the rendered frame differs from the previous frame. Cursor-position-only updates (same cell paint, different CUP or cursor show/hide CSI sequences) are rate-limited by a minimum interval so the stream does not flood on caret motion alone.

## Running mode: User prompt strip (activity pane)

In `AppMode::Running`, when `ViewState::running_input` is non-empty, the **last line** of the activity log renders a context strip `> {text}` with **white** foreground (`Color::White`) and **dark grey** background (`Color::DarkGray`). The strip is painted after the activity `Paragraph` so streamed tool and agent lines sit above this anchor.

## Bottom chrome: Footer row

Vertical layout reserves **one** terminal row immediately below the prompt text block as `footer_bar`. An empty `Paragraph` clears that row so the bottom chrome has stable height for overlays and optional future footer content.

## Mouse mode: Enter control

When pointer reporting is active (`EnableMouseCapture`), the TUI draws **Enter** and (when the terminal is wide enough) **Stop** affordances to the right of the prompt text. The layout narrows `prompt_bar` and `footer_bar` by `right_chrome_reserve_cols(terminal_width)`: **four** columns (margin + Enter strip) on very narrow terminals, **eight** columns when there is room for Enter plus Stop (margin + Stop strip).

- **Layout**: `layout_chunks_with_inbox` splits the terminal into **eight** vertical regions: activity, spacer, dynamic (inbox / questions / slash menu), status bar, **one empty row**, optional debug log, prompt block, and footer. The Enter strip’s **top** aligns with the **first row below the status bar** (the separator band: that empty row plus any debug rows allocated above the prompt chunk). Its **height** spans that band through the **prompt text** lines and the **footer** row. When the prompt chunk has more than one row, the **bottom** row is a horizontal rule (`U+2500`); that rule row is excluded from prompt line count and from Enter height so the rule stays visually separate from the frame.
- **Geometry**: Width **three** columns. Horizontal origin: `prompt_bar.x + prompt_bar.width + ENTER_STRIP_MARGIN_COLS`. When the status bar has height zero, the strip starts at `prompt_bar.y` and spans prompt text lines plus footer only.
- **Label**: Light box-drawing characters (`U+250C`–`U+2518`, `U+2500`, `U+2502`). **U+23CE** (`⏎`) sits on the **first prompt text row** inside the strip (below the top border when a separator band exists). Widgets for status, debug, prompt, and footer render first; the overlay paints afterward in the reserved columns.
- **Hit-testing**: `packages/tddy-tui/src/mouse_map.rs` defines `enter_button_rect` to match `render::paint_enter_affordance`; left-clicks inside the rectangle map to the same intents as **Enter** (`key_event_to_intent`).
- **E2E / stable screens**: When the environment variable `TDDY_E2E_NO_ENTER_AFFORDANCE` is set to any value, `paint_enter_affordance` returns without writing overlay symbols (byte-level terminal tests and gRPC terminal streams avoid frame noise).
- **Narrow terminals**: If the prompt region is too narrow for the margin plus three columns, painting and hit-testing omit the Enter affordance.

## Mouse mode: Stop control

When **`right_chrome_reserve_cols`** includes the Stop strip (terminal wider than the full eight-column reserve), the TUI paints a second three-column strip **immediately to the right of** the Enter strip, separated by `STOP_STRIP_MARGIN_COLS` (one column).

- **Label**: Same light box-drawing frame as Enter; the key cell uses **U+25A0** (■) in **red** (`Color::Red`) instead of U+23CE.
- **Hit-testing**: `stop_button_rect` matches `paint_stop_affordance`; left-click maps to **`UserIntent::Interrupt`**, which the local event loop and `VirtualTui` handle by calling **`ctrl_c_interrupt_session`** (same as **Ctrl+C** / byte **0x03** over the terminal stream). The intent is not sent to the presenter.
- **Semantics**: **`ctrl_c_interrupt_session`** only **`kill_child_process`** (tracked agent/backend PID). It does **not** set the workflow **`shutdown`** flag — the TUI and presenter keep running; full teardown uses the **`ctrlc`** SIGINT handler or daemon shutdown.
- **E2E / stable screens**: The same `TDDY_E2E_NO_ENTER_AFFORDANCE` gate skips both Enter and Stop overlay paint.
- **Narrow terminals**: If only the four-column Enter reserve fits, `stop_button_rect` is empty and the Stop affordance is omitted.

## Operational notes

- Status-bar policy helpers live in `packages/tddy-tui/src/status_bar_activity.rs`; formatting helpers in `packages/tddy-tui/src/ui.rs`; frame composition in `packages/tddy-tui/src/render.rs`.
- Markdown scroll bounds for the plan viewer use `Paragraph::line_count` with the same wrap settings as `draw()`, behind the ratatui `unstable-rendered-line-info` feature, so line counts match what the widget paints.
- Per-frame diagnostic detail uses `log::trace!` on the hot path; presenter lifecycle messages truncate long ids in debug logs.
- Direct `println!` / `eprintln!` are not used on TUI draw paths (ratatui display integrity).

## Related documentation

- [gRPC remote control](grpc-remote-control.md) — terminal streaming and Virtual TUI
- [Coder overview](1-OVERVIEW.md) — TUI capability summary
