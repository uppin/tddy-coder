# 2026-10-24 — No terminal history offer while a full-screen TUI owns navigation

**Type:** Fix · **PR:** #714 (`#claude-fullscreen` 2/2, on #713)

Once #713 made Claude run its fullscreen renderer, the live terminal's **Load earlier output** /
**View history** offer was redundant: Claude scrolls its own transcript with the wheel. It was also
misleading, because alternate-screen repaints never become scrollback, so the history replay held
frames rather than a transcript.

`GhosttyTerminalSession` now withholds both while the live program owns navigation: it is on the
alternate screen, or it tracks the mouse. It re-reads that from the terminal after every chunk that
switches a DEC mode or hard-resets the terminal (`decModeChanges`, which now also reports `ESC c`
as `RIS`). **Back to live** is never withheld.

- Docs: [terminal-replay-lazy-scroll.md](../../../../docs/ft/web/terminal-replay-lazy-scroll.md)
  (scope, `GhosttyTerminalSession` contract, acceptance criterion 14)
- Tests: `TerminalHistoryPaging.cy.tsx`, "while a full-screen TUI owns navigation" (5 specs)
- Deferred: [a reattach to an alternate-screen TUI without mouse tracking](../../../../docs/dev/todo/2026-10-24-reattach-does-not-replay-the-alternate-screen.md)
- `oversized-file-ghostty-terminal-session`: 971 → 992 lines, under the deferral #713 recorded
