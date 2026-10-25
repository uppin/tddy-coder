# 2026-10-25 — Terminal buffer-text poll no longer pins the WebView

**Type:** Fix · **Packages:** tddy-web

## Problem

Tddy Desktop's WebView process (`tauri://localhost`) sat at a constant ~100% CPU with no terminal
output arriving. A `sample` of the WebContent process put ~88% of main-thread time in one DOM
timer callback building strings with `String.fromCodePoint` and resolving rope strings.

That callback is `GhosttyTerminalSession`'s 200 ms mirror poll, which runs in production and calls
`GhosttyTerminalHandle.getBufferText()` on both the live terminal and the history **page**
terminal (`PAGE_SCROLLBACK = 50000`). Each call serialized the whole buffer — scrollback included —
cell by cell through the WASM boundary, concatenating with `+=`, five times a second per mounted
session, whether or not anything had changed.

## Change (State A → State B)

- **A:** `getBufferText()` re-reads every line on every call.
- **B:** `GhosttyTerminal` keeps a content revision, bumped on every `write`/`clear` (the handle's
  and the `initialContent` writes). `getBufferText()` returns its last result when the terminal
  instance, revision and grid size (`cols`×`rows`, so a reflowing resize re-reads) are unchanged, and
  joins lines with an array instead of `+=` when it does re-read.

The hidden `terminal-buffer-text` / `terminal-older-buffer-text` mirrors and the 200 ms poll are
unchanged, so every spec that reads them keeps its contract; an idle terminal's poll is now a
cache hit, and React bails out of `setBufferText` on the identical string.

## Alternatives considered

- **Gate the poll on `showBufferTextForTest`** — rejected: component and e2e specs read the mirrors
  through higher-level panes (`SessionMainPaneRoomTerminal`, `GrpcSessionTerminalLazyHistory`,
  e2e stories) that never set it, and a production-off/test-on switch is a test-only branch.
- **Drop the DOM mirrors for an imperative test hook** — larger change to ~15 consumers; still open
  if the poll itself is ever worth removing.

## Tests

- `cypress/component/GhosttyTerminalBufferText.cy.tsx` (new): output written after a read appears
  in the next read; 100 polls of an unchanged 20 000-line terminal cost less than one full read
  (fails on master: 245 ms vs 4 ms).
