# Changeset: ReadSessionTerminal answers the screen's styles on request

**Created:** 2026-10-29
**Status:** 🚧 Pass 2: contract and failing tests (no behaviour yet)
**Type:** Feature
**Complexity**: 3. It spans three packages, two of them owning new surface. It adds no proto, wiring or registration, because the tool and its port exist (`2/8`). The risk is the highlighted-row rule.
**Stack:** `#home-inbox 9/9`, branch `feature/home-inbox/terminal-styles`, base `feature/home-inbox/question-answer`
**PRD:** [PRD-2026-10-29-terminal-styles.md](../../ft/daemon/1-WIP/PRD-2026-10-29-terminal-styles.md)

## Initial Discovery

[2026-10-29-home-inbox-terminal-styles-initial-discovery.md](./2026-10-29-home-inbox-terminal-styles-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [`packages/tddy-task/docs/code-issues/oversized-file-terminal-capture.md`](../../../packages/tddy-task/docs/code-issues/oversized-file-terminal-capture.md) | ⚠ During. `terminal_capture.rs` is 728 production lines, and this node adds the span extraction. Put it in a new sibling module (`terminal_capture/styles.rs`) so the file does not grow. Nobody has claimed it. |
| [`packages/tddy-tool-engine/docs/code-issues/oversized-file-project-home-tools.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-project-home-tools.md) | Unrelated. This node edits `project_home_terminal_tools.rs`, not `project_home_tools.rs`. |
| [`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-exec-tool-project-home.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-exec-tool-project-home.md) | ⚠ During, only if the port signature changes. The preferred shape leaves `read_terminal` alone (see Responsibility). |

## Affected Packages

- [ ] [`tddy-task`](../../../packages/tddy-task/README.md): style runs and highlighted rows from the screen model's cell pens
- [ ] [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md): `ReadSessionTerminal`'s `styles` argument, its answer fields and its description
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): acceptance tests through the real tool. Code changes only if the port needs them.

## Responsibility

This node owns:

- **In `tddy_task`:**
  - `StyleRun { from, to, fg, bg, bold, dim, italic, underline, inverse, strikethrough, blink }`, with only the set attributes serialised;
  - `RowStyles { row, runs }`;
  - the colour naming (`TerminalColour`: named 0–15, `idx:N`, `#rrggbb`);
  - the extraction from `avt` lines, over characters with wide tails skipped, so columns match `TextGrid.lines`;
  - `highlighted_rows(&[RowStyles], &[String]) -> Vec<u16>`, by the rule pinned under Decisions.
- **Grid shape (confirmed in pass 2):** `TextGrid` gains `styles: Vec<RowStyles>` and `widths: Vec<Vec<u8>>`, both filled by `text_grid()` from one walk over the screen model's cells (`walk_cells`). It is cheap: one pass over a visible grid of at most a few thousand cells. `ProjectHomeTerminalPort::read_terminal` and both its impls stay unchanged.
  - The only struct literal this broke is the fake in `packages/tddy-tool-engine/tests/project_home_terminal_tools.rs`, fixed with `styles: Vec::new(), widths: Vec::new()`.
  - `widths` is each character's cell span (1 or 2). A crop is in screen cells and a row's text is characters, so the cut needs it. An empty `widths` is read as all narrow.
- **`ReadSessionTerminal`:**
  - the `styles` boolean property;
  - `grid_answer` adds `styles` and `highlighted_rows` only when it was asked for;
  - the description says when to ask.
- **Crop box (added 2026-10-29 at the developer's request):**
  - **The cropping step:** `tddy_task::GridRegion { row, col, rows, cols }`, and a pure `crop(&TextGrid, GridRegion) -> Result<CroppedGrid, String>`.
  - **What `crop` does:**
    - cuts `lines` on screen-cell boundaries, turning a wide character cut by an edge into a space;
    - clips and re-bases the style runs to cropped character columns;
    - makes the cursor box-relative, with `inside`;
    - clips a box that overruns the screen;
    - refuses a zero-size or wholly-off-screen box, naming the screen size.
  - **Highlighted rows** are recomputed on the cropped rows.
  - **The tool side:** `ReadSessionTerminal` takes an optional `region` object, and its answer echoes the effective `region`.
  - **Where it runs:** cropping happens in the tool engine after `read_terminal`, so the port stays unchanged and a cross-host read crops on the owning host. That is cheap, because the whole visible grid is already in hand.

## Boundaries

- No raw bytes, no scrollback, no snapshot-replay change, no `SendSessionKeys` change.
- No change to how `2/8` resolves, forwards or refuses a target.
- No change to the browser terminal.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#home-inbox 2/8` terminal-read (#767) | `TerminalCapture::text_grid`, `TextGrid`, `ReadSessionTerminal`, the `ProjectHomeTerminalPort` and its daemon impl, cross-host forwarding | extends the grid with style runs and the tool with one argument | change target resolution, refusals or forwarding |
| `#home-inbox 5/8` terminal-keys (#770) | `SendSessionKeys` | none: the agent reads styles before it types | change typing |

**Placement.** The only real edge is `2/8`, so the node is greenable at once. It sits at the top of
the line because it was added after the stack was planned, which the developer chose over reopening
`2/8`. The cost is that it merges only after `8/8`.

## Draft PR contract

Pass 2 publishes the surface below with bodies `// TODO(terminal-styles): implement`, and the failing
tests further down. The stubs fail closed: no runs, no highlighted rows, no widths, and `crop`
refuses every box with `Err("not implemented")`. Nothing panics.

**`tddy-task`** (new `terminal_capture/styles.rs` and `terminal_capture/crop.rs`; `terminal_capture.rs` gains only the `mod`, re-exports, two `TextGrid` fields and one call):

- `TerminalColour { Indexed(u8), Rgb(u8, u8, u8) }` with `Display`: the 16 standard indices as `black … white` and `bright-black … bright-white`, other indices as `idx:N`, true colour as `#rrggbb`.
- `StyleRun { from, to, fg, bg, bold, dim, italic, underline, inverse, strikethrough, blink }`: `from` inclusive, `to` exclusive, character positions in the row's `lines` entry.
- `RowStyles { row, runs }`.
- `highlighted_rows(&[RowStyles], &[String]) -> Vec<u16>`.
- `TextGrid.styles`, `TextGrid.widths`.
- `GridRegion { row, col, rows, cols }`, `CroppedCursor { row: i32, col: i32, visible, inside }`, `CroppedGrid { lines, styles, highlighted_rows, cursor, region }`.
- `crop(&TextGrid, GridRegion) -> Result<CroppedGrid, String>`.

**`tddy-tool-engine`:** `ReadSessionTerminal` takes `styles` (boolean) and `region` ({row, col, rows, cols}). `grid_answer` adds `styles` and `highlighted_rows` only with `styles: true`, and with a `region` answers the cropped `lines`, `cursor` and style fields plus an echoed `region`; `cols`, `rows` and `offset` stay the whole screen's. `run_answer` (the span JSON) is a stub that writes only `from` and `to`.

**Span JSON, pinned:** `styles: [{"row": 1, "runs": [{"from": 0, "to": 9, "inverse": true}]}]`. Only set attributes appear: `fg` and `bg` as colour names, flags as `true`. Rows with no run have no entry.

## Green wave

**Wave:** 5 by position. Its real need is wave 1 (`2/8`).
**Greenable independently:** yes. It needs only `2/8`'s behaviour, which has landed.
**Concurrent with:** any node.
**Blocks:** nothing.

## State A

`ReadSessionTerminal` answers `lines`, `cursor`, `cols`, `rows` and `offset`, which is text only.
`TerminalCapture` keeps an `avt::Vt` whose cells each carry a `Pen`. `text_grid()` reads
`line.text()` and drops the pens.

## State B

`ReadSessionTerminal { session_id, styles: true }` also answers `styles` (the non-default runs per
row) and `highlighted_rows`. Without `styles`, the answer is unchanged.

## Testing Strategy

Every acceptance test fails on the pass-2 stubs. Of the unit tests, the negative highlight cases
pass today because the stubs fail closed; they are guards and must keep passing.

### Acceptance tests (pass 2)

In `packages/tddy-daemon-rpc/tests/project_home_terminal_styles_acceptance.rs`. They use the real
`ReadSessionTerminal` over a session whose terminal is fed real SGR bytes by a shell script.

| # | Test | Checks | Fails today with |
|---|---|---|---|
| 1 | `an_inverse_video_menu_row_is_highlighted_...` | the row is in `highlighted_rows`; one run `{0, 9, inverse}` | `highlighted_rows` is `[]`, expected `[1]` |
| 2 | `a_background_bar_wider_than_its_text_...` | row highlighted; run `{0, 20, bg: "blue"}` past the 11-character text | the same |
| 3 | `a_red_word_and_a_bold_word_...` | runs `{8, 13, fg: red}` and `{18, 25, bold}`; row not highlighted | rows with runs is `[]`, expected `[0]` |
| 3b | `a_red_word_inside_an_inverse_bar_...` | three touching inverse runs, the middle one red; row highlighted | rows with runs is `[]` |
| 4 | `a_read_without_styles_...` | a plain read and `styles: false` are the five text fields and equal; a styled read keeps them and adds `styles` and `highlighted_rows` | the styled half: rows with runs is `[]`, expected `[1]` |
| 5 | `a_styled_read_of_a_session_on_another_host_...` | a styled read through `daemon_instance_id` gives the same highlight and run | `highlighted_rows` is `[]` |
| 6 | `a_cropped_styled_read_...` | box `{1, 30, 3, 20}` of a two-pane screen: the box's text, the run re-based to `{0, 9, inverse}`, the row highlighted inside the box though not on the whole screen, region echoed | `ReadSessionTerminal: not implemented` |
| 7 | `a_cropped_read_without_styles_...` | exactly `cols cursor lines offset region rows`; cursor `{1, 2, inside}` and `{-1, -30, outside}` | `ReadSessionTerminal: not implemented` |
| 8 | `a_box_past_the_screen_is_clipped_...` | an overrunning box is clipped and echoed; zero-size and off-screen boxes refused naming `220 columns by 24 rows` | `ReadSessionTerminal: not implemented` |

### Unit tests (pass 2)

- `packages/tddy-task/tests/terminal_styles.rs` (25 tests; 17 fail, 8 pass as fail-closed negative cases):
  - colour naming: the 16 standard indices, `idx:N`, `#rrggbb`, and the colours read back from a screen;
  - each attribute as its own flag;
  - same-pen cells merge; different pens stay separate and touch; default cells make no run;
  - a bar's run reaches past the trimmed text;
  - wide characters keep later columns aligned;
  - the highlight rule: full cover, a bar wider than the text, leading blanks, partial cover, a bar starting late, a blank row, a coloured word inside an inverse bar, a bg bar split by a coloured word, a plain gap, different backgrounds, inverse beside bg, an ordinary row with coloured words, a colour without bar attributes, ascending order.
- `packages/tddy-task/tests/terminal_crop.rs` (18 tests, all fail with `not implemented`):
  - cell-boundary cuts, trimmed lines, a wide character at the left edge, at the right edge, and inside the box;
  - runs clipped at both edges and re-based, a run outside the box dropped, rows numbered from the box's top, `highlighted_rows` recomputed;
  - cursor inside, above-left (negative), below-right, on the last cell versus one past it, and hidden;
  - clip versus each refusal, and a box that exactly fits.

## Decisions & Trade-offs

- **Opt-in, not always-on.** A styled answer is several times larger. Most reads only need the text.
- **`highlighted_rows`, not `selected_rows`.** The rule cannot tell a selection from a status bar.
  The name says what it measures, and the description says what it usually means.
- **The highlight rule (relaxed from "one run").** A row is highlighted when it has a non-blank
  character, and the runs covering the span from its first to its last non-blank character leave no
  default-styled gap, and either all are `inverse` or all share one identical `bg`. A coloured word
  inside an inverse bar still counts. A row with no non-blank character is never highlighted.
- **Column systems.** Runs are in character positions (they slice `lines`); a crop box is in screen
  cells. `TextGrid.widths` bridges them, and both come from one `walk_cells`.
- **Crop.** It runs in the tool engine after `read_terminal`, so the port is unchanged and a
  cross-host read crops on the owning host. A cut wide character becomes a space and keeps its pen.
  The cropped cursor is signed (negative above or left of the box). A refusal names `<cols> columns
  by <rows> rows`.
- **JSON stays in the tool engine.** `tddy-task` has no `serde`, and adding it for one shape is a
  new dependency, so `StyleRun` is a plain struct and `run_answer` builds the JSON.
- **A follow-up node, not a `2/8` amendment.** `2/8` is wrapped and green, and amending it would
  rebase six PRs.

## TODO

- [x] Pass 2: the surface and the failing acceptance and unit tests (contract commit)
- [ ] TDD Green
- [ ] `/validate-changes`, `/pr-wrap`

## Validation Results

Not yet run. Scoped gates at the contract commit: `cargo clippy --all-targets -D warnings` and `cargo fmt` clean on `tddy-task`, `tddy-tool-engine` and `tddy-daemon-rpc`; the tests above fail as listed.
