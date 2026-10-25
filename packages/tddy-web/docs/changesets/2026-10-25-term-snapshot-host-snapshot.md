# 2026-10-25 — Host terminals open with a screen snapshot over bidi `StreamSessionTerminalIO`
**Type:** Fix

`#term-snapshot` 1/4 ([#760](https://github.com/uppin/tddy-coder/pull/760)).

- `GrpcSessionTerminal` and `openDaemonTerminalFeed` open one bidi `streamSessionTerminalIO` call
  per open, with `mode = SNAPSHOT` every time (first connect, reconnect, remount). The opening message
  carries identity, control token and grid; each `send` is one stdin message with a cumulative
  `inputOffset` and the control token read at that instant (no batching). They no longer use
  `streamTerminalOutput` / `sendTerminalInput` or the `TAIL` → `FROM_OFFSET` resume.
- `TerminalFrame` gains `snapshot` and `startOffset`; `TerminalStreamOffset` takes the absolute
  `endOffset` of every frame of a snapshot stream.
- `InputRefusalTracker` (`src/rpc/connections/inputDelivery.ts`) maps refusal frames to the
  "Input not delivered" indicator, cleared by an ACK past the refused offset.
- Regenerated `src/gen/terminal_session_pb.ts`.

Tests: `cypress/component/TerminalSnapshotOnReconnect.cy.tsx` (absorbs the "onDisconnect is not
called on a blip" case of the deleted `GrpcSessionTerminalResume.cy.tsx`);
`src/rpc/connections/terminalFeed.test.ts` rewritten for the bidi wire; the
`GrpcSessionTerminal*`, `TerminalInputAckAcceptance`, `ConnectedSessionInputGating` and
`SessionsDrawerVisibleErrorsAcceptance` specs and `terminalSessionServiceBackend.ts` moved onto the
bidi stream.

Deferred: `TerminalResumePoint`, `onOffsetUpdate`, the duplicated request queue and
`inputDeliverySequence` (backlog `2026-10-25-web-host-terminal-feed-leftovers-after-snapshot`).

See [terminal-session.md](../terminal-session.md).
