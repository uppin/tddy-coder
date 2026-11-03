# 2026-11-03 — A terminal reports ready only once its initial content is written

**Type:** Bug Fix

`GhosttyTerminal` writes the `initialContent` present at open (and taps it for OSC 52) in the open path, before `setReady(true)` and `onReady()`; `writtenInitialContentRef` lets the `[initialContent, ready]` effect skip a value already written while it still writes a later change. `onReady` and `data-terminal-ready` therefore mean "showing what it was given": `DormantAgentTerminal`'s history logic no longer runs against an empty buffer, and specs that act right after ready (#815, #816 waits; Dpr, Isolation, Osc52Acceptance, TerminalTapMouseClick) no longer race the write. The test-only waits stay. Test: `GhosttyTerminal.cy.tsx` "holds its initial content by the time it reports ready". Code issues: `oversized-file-ghostty-terminal` 1475 → 1491 (+16), deferred. Part of [#818](https://github.com/uppin/tddy-coder/pull/818).
