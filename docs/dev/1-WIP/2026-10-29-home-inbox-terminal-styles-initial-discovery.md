# Initial discovery: terminal-styles (`#home-inbox 9/9`)

## Conclusions

- `TerminalCapture` (`packages/tddy-task/src/terminal_capture.rs`) holds `screen: avt::Vt`, built by `new_screen` (:710).
- `text_grid()` (:141) maps `self.screen.view()` through `line.text().trim_end()`, which drops every cell's pen.
- **`avt` 0.18 keeps the styles on every cell:**
  - `Line::cells() -> &[Cell]`, and `Cell::pen() -> &Pen`.
  - `Pen` carries `foreground()` / `background()` as `Option<Color>` (`Color::Indexed(u8) | Color::RGB(RGB8)`).
  - It reports `is_bold`, `is_faint`, `is_italic`, `is_underline`, `is_strikethrough`, `is_blink` and `is_inverse`.
  - `Line::chunks(predicate)` groups cells, skipping wide tails. `Line::text()` also skips wide tails, so columns built over the same iteration match `TextGrid.lines`.
- **`ReadSessionTerminal`** (`packages/tddy-tool-engine/src/project_home_terminal_tools.rs`):
  - Its def (:55) takes `session_id` plus the target-host property.
  - `execute_project_home_terminal_tool` (:168) resolves the target, calls `port.read_terminal(&target) -> anyhow::Result<TextGrid>` (:125), and answers through `grid_answer` (:202).
- **The daemon impl** (`packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`, about line 692) locks the session's main terminal capture and calls `text_grid()`.
- **Cross-host reads** forward the whole tool call to the owning daemon. A new argument travels with it, and the peer runs the same code.
- **Code issues in the path:**
  - `tddy-task` `oversized-file-terminal-capture.md` (728 lines, unclaimed);
  - `tddy-daemon-rpc` `oversized-file-exec-tool-project-home.md` (unclaimed);
  - none are claimed by another stack.
- **`docs/dev/todo/`:** no entry covers terminal styles for the agent.

## Exploration 1 (2026-10-29)

These were read on `origin/feature/home-inbox/question-answer` (`5f3bd0ee9`):

- `project_home_terminal_tools.rs`: lines 55–70, 120–130 and 168–225
- `exec_tool/project_home.rs`: lines 690–715
- `terminal_capture.rs`: the `TextGrid`, `GridCursor` and `text_grid` sections
- `avt-0.18.0/src/{pen.rs, color.rs, line.rs, vt.rs}` from the local crates.io proxy cache
