# 2026-10-29 — ReadSessionTerminal styles and crop box

**Type:** Feature

`#home-inbox` 9/9 ([#785](https://github.com/uppin/tddy-coder/pull/785)). The project-home agent reads
a terminal's colours and attributes, and one box of the screen. Across:

- `tddy-task` — `TextGrid` gains `styles: Vec<RowStyles>` and `widths: Vec<Vec<u8>>`, both filled by
  one walk over the screen model's cells (`terminal_capture/styles.rs`, `walk_cells`).
  `StyleRun`, `TerminalColour` (named 0–15, `idx:N`, `#rrggbb`), `highlighted_rows`, and a pure
  `crop(&TextGrid, GridRegion) -> Result<CroppedGrid, String>` (`terminal_capture/crop.rs`).
  `TextGrid` and `GridCursor` moved to `terminal_capture/grid.rs`. 43 unit tests
  (`terminal_styles`, `terminal_crop`).
- `tddy-tool-engine` — `ReadSessionTerminal` takes `styles` and `region`; `grid_answer` adds the
  style fields only on request and crops after the port read. The port is unchanged, so a cross-host
  read crops on the owning host.
- `tddy-daemon-rpc` — 9 acceptance tests through the real tool (`project_home_terminal_styles_acceptance`).

Decisions: opt-in, since a styled answer is several times larger; `highlighted_rows` not
`selected_rows`, because the rule cannot tell a selection from a status bar; a row is highlighted
when the runs over its non-blank span leave no default gap and are all inverse or share one
background, so a coloured word inside a bar still counts; JSON stays in the tool engine because
`tddy-task` has no `serde`; wide tails are skipped by `Cell::width() == 0`, the selection
`Line::chars()` makes; a crop pads each row to the screen width and trims after the cut, so no
trailing blanks reach `lines`.

Code issues: `terminal_capture.rs` 728 → 712 production lines (the grid types moved out, offsetting
the module wiring and two fields). Known red, not this PR's: `read_only` unit test expects no
`AnswerSessionQuestion`, owned by `#home-inbox 8/8`.
