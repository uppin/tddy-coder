# 2026-11-08 — The terminal components split along their input, scroll and history seams (#xterm 1/4, PR #866)

**Type:** Refactor

- `GhosttyTerminal.tsx` (1491 → 199 production lines) is a composition of hooks in `src/terminal/`;
  `GhosttyTerminalSession.tsx` (1004 → 486) a composition of hooks and pane components in
  `src/session/`. The largest new file is `src/terminal/useTerminalLifecycle.ts` (277 lines).
- Mechanical moves only: the exports of `src/index.ts`, the props, the test ids and every Cypress
  spec are unchanged.
- `src/componentSize.test.ts` fails when a production source file exceeds 500 lines; test files are
  outside the budget.
- Closed with their final measurement: the `oversized-file-ghostty-terminal` record (1491 → 199) and
  the `oversized-file-ghostty-terminal-session` record (1004 → 486), and the backlog entry
  `2026-10-24-ghostty-terminal-components-over-budget`.
- Docs: [README.md](../README.md) § Source layout.
