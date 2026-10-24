# Changeset: the terminal hides its history affordances while a full-screen TUI owns navigation

**Date:** 2026-10-24
**Type:** Fix
**Branch:** `feature/claude-fullscreen/hide-history-for-tuis` — stacked on `claude-cli-fullscreen` (#713)
**Packages:** `tddy-web`

## Summary

The live terminal offers **Load earlier output** / **View history**, which swap the live pane for a
replay of the session's recorded bytes. Once #713 makes Claude run its fullscreen renderer, that offer
is redundant and misleading: Claude scrolls its own transcript with the wheel, and its output is
alternate-screen repaints, which a terminal never keeps as scrollback, so the history replay holds
frames rather than a transcript.

While the live terminal's program **owns navigation** — it is on the alternate screen (DEC `1049` /
`1047` / `47`) **or** it has mouse tracking on (DEC `1000` / `1002` / `1003`) — neither affordance is
shown. They come back as soon as it gives both up (quitting the TUI back to a shell).

Why both signals: on reattach the daemon re-sends the mouse modes the program enabled
(`TerminalCapture::mode_prologue`) but not the alternate screen, so a browser joining an
already-running fullscreen Claude learns about mouse tracking and nothing else. A pager like `less`
switches to the alternate screen without mouse tracking and also scrolls itself (ghostty-web turns the
wheel into arrow keys there).

## Responsibility

- `GhosttyTerminalSession` tracks whether the live terminal's program owns navigation, recomputed
  after every written chunk that switches a DEC private mode or hard-resets the terminal (`ESC c`),
  including chunks buffered before the terminal was ready.
- `Load earlier output` and `View history` are not rendered while it does. `Back to live` is never
  hidden — a user already viewing history must always be able to return.
- Cypress component coverage in `TerminalHistoryPaging.cy.tsx`.

## Boundaries

- No daemon change: the mode prologue is **not** extended to re-send `1049`. Mouse tracking already
  covers the reattach case for Claude; a reattach to `less` keeps showing the affordances. Recorded
  below as a known gap rather than fixed here.
- The history double buffer, its fetch and its swap are unchanged.

## Dependencies

- **#713 (`claude-cli-fullscreen`)** — the `decModeChanges` / `traceModes` helpers in
  `GhosttyTerminalSession.tsx`, which this node reuses to notice mode switches. Nothing here is needed
  for #713's own behaviour; #713 is what makes the problem visible (Claude is now fullscreen).

## Draft PR contract

None — no successor consumes a surface from this node. Behaviour and tests land together.

## Green wave

**Wave:** 1 of 1
**Greenable independently:** yes — the specs mount `GhosttyTerminalSession` with an in-memory feed
and drive real escape sequences through it; nothing from #713's daemon change is exercised.
**Concurrent with:** #713
**Blocks:** nothing

## Known gap

A browser reattaching to a TUI that uses the alternate screen **without** mouse tracking (`less`,
`man`) is not told about the alternate screen, so it shows the affordances until that program next
switches modes. Closing it means adding `1049`/`1047`/`47` to `TerminalCapture`'s replayed prologue.

## Acceptance criteria

- [x] Entering the alternate screen hides **Load earlier output**; leaving it shows it again.
- [x] Mouse tracking on the normal screen hides **Load earlier output**.
- [x] After a fill, entering the alternate screen hides **View history**.
- [x] Entering the alternate screen while viewing history keeps **Back to live**.
