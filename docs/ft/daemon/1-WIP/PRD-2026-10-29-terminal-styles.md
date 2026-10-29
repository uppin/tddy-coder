# PRD: The project-home agent sees what a terminal highlights

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
  - **`highlighted_rows`:** the rows where a single inverse or background-coloured run covers every
    non-blank character of the row. That is what "selected" usually looks like. Status bars
    qualify too, which the tool description says.

Columns are character positions in the row's `lines` string, so a run slices that string directly.
A run may extend past the trimmed text into the blank cells a highlight bar paints, up to `cols`.

Colours are named for the 16 standard indices (`red`, `bright-blue`, ...), `idx:N` for the
256-colour palette, and `#rrggbb` for true colour.

The tool description tells the agent when to ask for styles: when it must know what is selected
before it types.

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
   highlighted.
4. A read without `styles` answers exactly what `2/8` answers.
5. A styled read of a session on another host, through `daemon_instance_id`, answers the same.
6. Wide characters (CJK, emoji) do not shift the columns of the runs after them.
