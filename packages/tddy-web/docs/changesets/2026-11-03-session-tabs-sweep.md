# 2026-11-03 — Initial content writes once; the tmux menu reports a missing host

**Type:** Bug Fix

`GhosttyTerminal` writes `initialContent` (and taps it for OSC 52) only in its `[initialContent, ready]` effect; the setup effect's duplicate write is gone. `useTmuxSessions` reports `hostNotConnectedText(null)` with `loading: false` while a listing is asked for with no client, and lists when the client arrives. Tests: `GhosttyTerminal.cy.tsx`, `GhosttyTerminalOsc52Acceptance.cy.tsx` (the driver's `expectBufferHoldsTimes`), `UseTmuxSessionsHostNotConnected.cy.tsx`. Code issues: `oversized-file-ghostty-terminal` 1479 → 1475, `oversized-file-session-main-pane` unchanged (504), both deferred. Part of [#813](https://github.com/uppin/tddy-coder/pull/813).
