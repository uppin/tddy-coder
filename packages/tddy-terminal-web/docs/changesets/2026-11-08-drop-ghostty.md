# 2026-11-08 — ghostty-web is removed; the components are TerminalView and TerminalSession (#xterm 4/4, PR #869)

**Type:** Refactor

`ghostty-web`, `src/engine/ghosttyEngine.ts`, `src/lib/ghosttyInstance.ts` and
`src/lib/terminalDprWatch.ts` are deleted; `xtermEngine` is the only `TerminalEngine`.
`GhosttyTerminal` → `TerminalView`, `GhosttyTerminalSession` → `TerminalSession`, with
`TerminalViewProps`/`TerminalViewHandle`/`TerminalViewTheme` and `TerminalSessionProps`;
`TEST_IDS.ghosttyTerminal` → `TEST_IDS.terminal` (`terminal`, `terminal-older`, `terminal-dormant`).
Specs and drivers are renamed to match (`TerminalSession*.cy.tsx`, `terminalViewDriver`,
`terminalSessionDriver`). `docs/terminal-engine.md` describes the xterm.js mapping. No behaviour
change.
