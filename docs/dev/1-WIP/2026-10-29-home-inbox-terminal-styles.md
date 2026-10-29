# Changeset: ReadSessionTerminal answers the screen's styles on request

**Created:** 2026-10-29
**Status:** 🚧 Planned (pass 1: docs only)
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
  - `highlighted_rows(&[RowStyles], &[String]) -> Vec<u16>`.
- **Preferred shape:** `TextGrid` gains `styles: Vec<RowStyles>`, filled by `text_grid()`. It is cheap: one pass over a visible grid of at most a few thousand cells. Then `ProjectHomeTerminalPort::read_terminal` and both its impls stay unchanged.
  - Pass 2 confirms or rejects this against the struct literals it breaks. Those are test fakes in `2/8` and `5/8`; the alternative is a separate method.
- **`ReadSessionTerminal`:**
  - the `styles` boolean property;
  - `grid_answer` adds `styles` and `highlighted_rows` only when it was asked for;
  - the description says when to ask.

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

Pass 2 publishes the surface above with bodies `// TODO(terminal-styles): implement`, and the
failing tests below. The stubs fail closed: `styles` is empty, and `highlighted_rows` is empty.

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

### Acceptance tests (pass 2)

These go in `packages/tddy-daemon-rpc/tests/project_home_terminal_styles_acceptance.rs`. They use
the real `ReadSessionTerminal` over a session whose terminal is fed real SGR bytes:

1. An inverse-video menu row: the row is in `highlighted_rows`, and its run is `inverse` over the row's columns.
2. A background-coloured bar wider than its text: the row is highlighted, the run carries `bg`, and `to` reaches past the trimmed text.
3. A row with a red word and a bold word: two runs, and the row is not highlighted.
4. The same session read without `styles`: the answer equals `2/8`'s shape exactly.
5. Cross-host: a styled read through `daemon_instance_id` answers the same.

### Unit tests (pass 2)

These go in `packages/tddy-task/tests/terminal_styles.rs`:

- the colour naming: named, indexed and RGB;
- wide characters keep later columns aligned;
- adjacent cells with the same pen merge into one run;
- default-styled runs are omitted;
- the highlighted-row rule: full cover, partial cover, a blank row, a row that is all spaces under a bar.

## Decisions & Trade-offs

- **Opt-in, not always-on.** A styled answer is several times larger. Most reads only need the text.
- **`highlighted_rows`, not `selected_rows`.** The rule cannot tell a selection from a status bar.
  The name says what it measures, and the description says what it usually means.
- **A follow-up node, not a `2/8` amendment.** `2/8` is wrapped and green, and amending it would
  rebase six PRs.

## TODO

- [ ] Pass 2: the surface and the failing acceptance and unit tests (contract commit)
- [ ] TDD Green
- [ ] `/validate-changes`, `/pr-wrap`

## Validation Results

Not yet run.
