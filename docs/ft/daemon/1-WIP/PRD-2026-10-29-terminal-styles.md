# PRD: The project-home agent sees what a terminal highlights, and reads part of it

**Created:** 2026-10-29
**Status:** 🚧 Planned
**Area:** daemon — project home (`docs/ft/daemon/project-concept.md`)
**Stack:** `#home-inbox 9/9`

## Problem

`ReadSessionTerminal` (`#home-inbox 2/8`) answers a session's screen as text only: "no scrollback,
colours or attributes". `SendSessionKeys` (`5/8`) then lets the agent act on that screen.

Many TUIs mark the current item **only** by style: an inverse-video bar (fzf, lazygit, htop), a
background-coloured row (menus, pickers), or a bold or coloured item with no glyph. On those
screens the agent cannot tell which item a key like Enter will act on. It guesses, and it now has
the means to type the guess.

Claude Code's own dialogs stay readable without this, because they mark the choice with `❯`. The
gap is every other TUI a session runs.

## What changes

`ReadSessionTerminal` gains an opt-in argument, `styles` (boolean, default `false`):

- **`styles` absent or `false`:** the answer is unchanged, byte for byte, so ordinary reads cost
  nothing more.
- **`styles: true`:** the answer adds two fields.
  - **`styles`:** for each row that has any styled text, the runs whose style differs from the
    terminal default. Each run names its columns and only the attributes that are set:
    foreground, background, bold, dim, italic, underline, inverse, strikethrough, blink.
  - **`highlighted_rows`:** the rows painted as a bar: the runs covering every non-blank character
    of the row are contiguous (no default-styled gap between them), and either all are inverse or
    all share one identical background colour. A coloured word inside an inverse bar still counts.
    That is what "selected" usually looks like. Status bars qualify too, which the tool description
    says.

Columns are character positions in the row's `lines` string, so a run slices that string directly.
A run may extend past the trimmed text into the blank cells a highlight bar paints, up to `cols`.

Colours are named for the 16 standard indices (`red`, `bright-blue`, ...), `idx:N` for the
256-colour palette, and `#rrggbb` for true colour.

The tool description tells the agent when to ask for styles: when it must know what is selected
before it types.

### Crop box

`ReadSessionTerminal` also takes an optional `region` argument: `{row, col, rows, cols}`. This is
the top-left cell, zero-based, plus a size, all in screen cells, the same units as `cursor`, `cols`
and `rows`. The agent uses it to read one pane, menu or dialog without the rest of the screen.

- **What a cropped read answers:**
  - `lines` holds only the box's rows, each cut to the box's columns. It is still trimmed at the end.
  - A wide character cut by either edge becomes a space.
- **Styles are kept.** With `styles: true`, the runs are clipped to the box and re-based:
  - row 0 and column 0 are the box's top-left;
  - columns are character positions in the cropped `lines` strings.
  - `highlighted_rows` is computed on the cropped rows, so a menu bar highlighted inside a pane
    counts even when the pane shares its row with other text.
- **Coordinates:** the answer echoes the effective `region` it read.
  - `cursor` is relative to the box, with `inside: false` when the cursor is outside it.
  - `cols` and `rows` stay the full screen size, so the agent can place the box.
- **Edges:**
  - a box running past the screen is clipped to it, and the echoed `region` shows the clip;
  - a box with zero size, or wholly off the screen, is refused, naming the screen size.
- **No `region`:** the whole screen, exactly as before.

## Non-goals

- Raw output bytes or escape sequences. The screen model already resolves them.
- Scrollback. The tool still reads only the visible screen.
- Changes to `SendSessionKeys`, the browser terminal, or the snapshot replay.

## Acceptance criteria

1. A session whose screen shows an inverse-video row answers that row in `highlighted_rows`, and
   its run as `inverse: true` with the row's columns.
2. A row painted with a background colour bar is in `highlighted_rows`. Its run carries `bg`, and
   reaches past the trimmed text when the bar does.
3. Coloured or bold words inside an ordinary row appear as runs but do not make the row
   highlighted. A coloured word inside an inverse bar does not stop the bar being highlighted.
4. A read without `styles` answers exactly what `2/8` answers.
5. A styled read of a session on another host, through `daemon_instance_id`, answers the same.
6. Wide characters (CJK, emoji) do not shift the columns of the runs after them.
7. A cropped read of a pane answers only the box's rows and columns, with its style runs clipped
   and re-based to the box. A highlighted menu row inside the box is in `highlighted_rows`, though
   the full-screen row holds other text.
8. A cropped read without `styles` answers only text, with the echoed `region` and a box-relative
   `cursor` (`inside` false when the cursor is outside the box).
9. A box past the screen edge is clipped, and the echo shows it. A zero-size or wholly-off-screen
   box is refused, naming the screen size.
10. A wide character cut by the box's edge becomes a space, and the columns after it stay aligned.
