# 2026-10-25 — Terminal continuity guard and gap resync
**Type:** Feature

`#term-snapshot` 2/4 ([#761](https://github.com/uppin/tddy-coder/pull/761)).

- `src/lib/terminalFrameContinuity.ts`: `TerminalFrameContinuity.accept(frame)` → `"paint" | "gap"`
  (a snapshot anchors; a delta must start at the previous `endOffset`; every delta after a gap is a
  gap until the next snapshot) and `route(frame)` → `"paint" | "drop" | "drop-and-ask"` (one request
  per run of gapped frames, re-armed by the answering snapshot).
- `openDaemonTerminalFeed` and `GrpcSessionTerminal` drop gapped frames and yield one data-free
  `requestSnapshot: true` message on the open bidi stream, with the current control token.
- Regenerated `src/gen/terminal_session_pb.ts`.

Tests: `src/lib/terminalFrameContinuity.test.ts` (8); `terminalFeed.test.ts` gains the feed's gap
case; `cypress/component/TerminalResyncOnGap.cy.tsx`. Fixtures that sent an unstamped first frame
(`terminalFeed.test.ts`, `GrpcSessionTerminalLazyHistory.cy.tsx`) now send it as the snapshot a
`SNAPSHOT` open receives.

The guard is wired in both host paths because those paths are still separate (backlog
`2026-10-25-web-host-terminal-feed-leftovers-after-snapshot`). See
[terminal-session.md](../terminal-session.md).
