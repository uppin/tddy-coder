# 2026-10-18 — Code pane index status strip
**Type:** Feature

`CodeIndexStatusStrip` along the bottom of `WorktreeCodePane` shows where the session's code index
stands — checking, the current phase with its percentage and the index server's own line, ready,
failed, or not warmed — and stays visible once ready. `useCodeIndexProgress` follows
`WatchCodeIndex(session)` as `waiting | progress | ready | failed | untracked` and is shared with
`SessionIndexingIndicator`, whose behaviour is unchanged. `WorktreeCodePane` takes an optional
`sessionId`, passed by `SessionMainPane`.

Docs: [session-start-and-indexing-progress.md § Indexing indicator](../session-start-and-indexing-progress.md#indexing-indicator),
[code-navigation.md](../code-navigation.md). Spec: `cypress/component/CodeIndexStatusStrip.cy.tsx` (3).

Code issues touched: `oversized-file-session-main-pane` 674 → 675 lines (+1, the `sessionId` prop);
`oversized-file-create-session-pane` unchanged at 795 (one default flipped in the same PR).
