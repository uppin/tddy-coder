# 2026-10-04 — Session start phases and code-index warm-up progress

**Type:** Feature

`#live-plan` 12/15 — PR [#571](https://github.com/uppin/tddy-coder/pull/571),
`feature/live-plan/indexing-indicators`. Cross-package entry:
[2026-10-04-indexing-indicators.md](../../../../docs/dev/changesets/2026-10-04-indexing-indicators.md). Product entry:
[2026-10-04-indexing-indicators.md](../../../../docs/ft/web/changelog/2026-10-04-indexing-indicators.md).

Every session start streams: `CreateSessionPane` always uses `startSessionStreamed`
(`StreamStartSession`), with or without attachments. `useSessionAttachments` exposes `startPhase`, which
follows the stream's `phase` events and is cleared when the stream ends however it ended;
`CreateSessionStartPhase` renders it under the form. `SessionIndexingIndicator` follows
`WatchCodeIndex` through the owning host's `CodeNavigationService` client and is rendered in
`SessionMainPane`'s header; it shows `Indexing — <phase> <n>%`, the failure's reason, and nothing once
ready or when nothing warms the session. Docs:
[session-start-and-indexing-progress.md](../session-start-and-indexing-progress.md).

**Test infrastructure.** `tddy-connectrpc-testkit` gains `registerServerStreamFallback`, registered in
`cypress/support/component.ts`, so specs that stub only the unary `startSession` keep working against the
streamed form; `cypress/component/ServerStreamFallback.cy.tsx` pins it. The testkit package has no
`docs/` directory, so it has no entry of its own; this entry and the cross-package entry carry it.

**Tests (scoped).** `ServerStreamFallback.cy` 7 passed, `CreateSessionPane.cy` 29 passed;
`SessionStartAndIndexingProgress.cy`, `CreateSessionAttachmentProgress.cy`, `CreateSessionAcceptance.cy`,
`CreateSessionBranchConflictAcceptance.cy`, `CreateSessionAutoClosesDrawer.cy`,
`PrStackStartSessionModalAcceptance.cy` and `CreateSessionCodebaseHostAcceptance.cy` passed. About 30
other specs that call `startSession` were left to CI.

**File length (deferred).** `CreateSessionPane.tsx` 783 → 785 lines and `SessionMainPane.tsx` 666 → 674:
code-issue records `oversized-file-create-session-pane` and `oversized-file-session-main-pane` opened;
the splits are deferred under the stack rule (PR #572 touches both files).
