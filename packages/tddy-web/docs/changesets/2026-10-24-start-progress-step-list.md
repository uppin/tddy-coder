# 2026-10-24 — The create pane lists each start step and a live clone bar

**Type:** Feature

`#start-progress` 7/8, PR [#740](https://github.com/uppin/tddy-coder/pull/740). See
[Session start progress](../session-start-and-indexing-progress.md#start-progress).

- `src/components/sessions/startProgress.ts` (new): `StartProgressState`, `NO_START_PROGRESS`,
  `startProgressAfter` (folds `phase` and `cloneProgress` events) and `startProgressFailed`.
- `CreateSessionStartProgress` (new) replaces `CreateSessionStartPhase`: "Creating session on <host>",
  one row per reported step (clone, worktree, semantic index, agent) marked active, done or failed, and a
  clone row with stage, percent bar, bytes and throughput through `formatAttachmentBytes`.
- `useSessionAttachments` returns `startProgress` instead of `startPhase`; a failed stream keeps the list
  with the active step `failed`, a created session clears it.
- `CreateSessionPane` resolves the host label (codebase host on a split start).
- Removed: `CreateSessionStartPhase.tsx`, `createSessionPage.startPhase`, test id `create-session-start-phase`.

Tests: `cypress/component/CreateSessionStartProgress.cy.tsx` (5); `SessionStartAndIndexingProgress.cy.tsx`
and `CreateSessionAttachmentProgress.cy.tsx` updated to the new page object.
