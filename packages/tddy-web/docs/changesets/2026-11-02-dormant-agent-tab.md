# 2026-11-02 — A stopped session's Agent tab shows its greyed output and offers Resume

**Type:** Feature

`#session-tabs 6/7` ([#809](https://github.com/uppin/tddy-coder/pull/809)). Reference: [inactive-session-activities.md](../inactive-session-activities.md).

- **`DormantAgentTerminal`** fills `DormantSessionRuntime`'s `agentTab` slot (from [#804](https://github.com/uppin/tddy-coder/pull/804)) in place of `SessionActivitiesPane`. It is a read-only `GhosttyTerminal` filled from `GetTerminalHistory(main)` to `atEnd` (served for a stopped session since [#805](https://github.com/uppin/tddy-coder/pull/805)), in `HISTORY_THEME` / `HISTORY_FILTER` (now exported from `GhosttyTerminalSession.tsx`), with "No recorded output" on `NOT_FOUND` or an empty recording.
  - `preventFocusOnTap={isMobile}` (accepted by the developer): a tap on a touch device does not raise the keyboard over a view that takes no input.
- **`Toast`** (`src/components/ui/toast.tsx`): a message, one action with a busy state, and an error rendered with `role="alert"`. No toast queue.
- **One resume action.** `useSessionsDrawerPaneActions.resume` resolves once attached and rejects with the failure. The toast shows the rejection; `handleResume` (top bar, inspector) drops it because the pane already shows it. `useSessionAttachment.resumeSession` now resolves the failure's text instead of a boolean.
- **Optimistic liveness.** `resumedSessions.ts` (pure) and `useResumedSessionLiveness` mark a resumed session live until a listing reports it live, so the pane goes live at once rather than at the 2s poll. Only `SessionMainPane` reads the hint.
- **Inspector Activity tab** (developer decision): `activity` in `INSPECTOR_TAB_NAMES`, `DORMANT_INSPECTOR_TAB_NAMES`, `InspectorTabs` `activityAvailable`, and `SessionInspectorActivityTab` rendering `SessionActivitiesPane`. Offered only while `canResumeSession(session)`; a live session's `?inspector=activity` degrades to Details. The n1 specs that pinned the transcript in the Agent tab were rewritten to this state, none deleted.

Backlog: the "~2s Resume dead time" bullet of [2026-08-01-tddy-web-inactive-session-activities-follow-ups.md](../../../../docs/dev/todo/2026-08-01-tddy-web-inactive-session-activities-follow-ups.md) is closed by the optimistic hint and removed; the record stays for its CSS-less-harness bullet.

## Restructuring (`/pr-wrap` step 3.5)

| File | Base → before wrap → after | Split into |
|---|---|---|
| `SessionInspectorDrawer.tsx` | 597 → 627 → **439** | `SessionInspectorDetailsTab.tsx` (metadata, traffic, lifecycle controls; `pendingDelete` stays in the drawer so a half-confirmed delete survives a tab switch), `SessionInspectorHeader.tsx` (title row, Expand / Restore / Close) |
| `SessionsDrawerScreen.tsx` | 508 → 521 → **443** | `sessionPaneClients.ts` (the per-service client choice, selected daemon while creating, owning host otherwise); `useResumedSessionLiveness` also returns `liveSelectedSession` |
| `SessionMainPane.tsx` | 503 → 504 | **Not split**: #810 also modifies it; recorded in `code-issues/oversized-file-session-main-pane.md` for the stack's sweep node |
| `GhosttyTerminalSession.tsx` | 1005 → 1005 | Unchanged size (exports only) |

Mechanical moves only; same scoped green before and after.

## Code issues

- `oversized-file-sessions-drawer-screen.md` — **closed and removed**: 508 lines at the base, **443** after the split (budget 500).
- `oversized-file-session-main-pane.md` — 503 → 504, row added; split deferred to the sweep node (stack-shared with #810).
- `oversized-file-ghostty-terminal-session.md` — unchanged at 1005, row added.
- `SessionInspectorDrawer.tsx` had no record at 597 lines; it is 439 now, so none is filed.

## Validation results

- validate-changes: on its base `feature/session-tabs/tab-plus` (`0426e14e2`), already current, leak-free (`origin/<base>..HEAD` is this PR's commits only). Responsibility delivered; nothing from Dependencies implemented here; no boundary crept in. One stale note fixed: the Cypress mount sites already pass `onResumeSession` and type-check.
- validate-tests: Given-When-Then added to `Toast.cy.tsx`; the raw `dormant-agent-*` selectors repeated across six specs moved into `cypress/support/pages/dormantAgentTabPage.ts` and `TEST_IDS`; the Activity-tab acceptance case reuses the spec's backend builder (`transcript: true`).
- validate-prod-ready: no mock code, TODO/FIXME, debug output or suppressions added. `isLive` was test-only; `withResumedLiveness` now uses it.
- analyze-clean-code: `DormantAgentTerminal` split into `useRecordedOutput` and `useResumeAction` (component body ~40 lines); `useResumedSessions.ts` renamed `resumedSessions.ts` (pure functions, not a hook); a duplicated JSDoc block above `HISTORY_THEME` merged.

Verified (scoped to `tddy-web`; the full Cypress suite was not run): `bun test src/components/sessions src/routing` 555 pass, 0 fail. Cypress component, 151/151: `DormantAgentTabAcceptance` 8, `Toast` 4, `DormantSessionTabsAcceptance` 11, `InactiveSessionActivitiesAcceptance` 18, `SessionInactiveInspectorOverlay` 6, `SessionInspectorAcceptance` 14, `PrStackViewRoutingAcceptance` 2, `SessionAgentAttachTabAcceptance` 14, `TerminalHistoryPaging` 27, `WorkflowChatResumeAcceptance` 2, `SessionsDrawerAcceptance` 23, `SessionsDrawerCrossHostAcceptance` 8, `SessionInspectorUrlStateAcceptance` 9, `SessionInspectorVncAcceptance` 5. `tsc`: no new error (718, all pre-existing).
