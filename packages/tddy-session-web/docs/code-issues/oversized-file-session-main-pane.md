# oversized-file: SessionMainPane.tsx

**Location:** `packages/tddy-session-web/src/SessionMainPane.tsx`
**Category:** oversized-file
**Detected:** 2026-11-02 by `/pr-wrap` step 3.5 file-length gate on PR #804 (`#session-tabs` 1/7)
**Metrics:** **504 lines** (non-test source, whole file) · budget 500 · **1.008× over**
**Thresholds breached:** length 504 > 500
**Status:** Open — unclaimed
**Deferred:** by the developer 2026-11-03 (#session-tabs sweep: code issues deferred, logic issues solved)

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-02 | 468 | `origin/master` (`e9d036e0c`), base of PR #804 |
| 2026-11-02 | 468 → 503 | +35 in PR #804: the stopped session's strip (`DormantSessionRuntime`) in place of the full-pane transcript, the agent-conversation layer and its per-session hosts, and the Files-tab Insert route to the stopped strip. **Not split**: `#session-tabs` 6/7 (#809) edits this same file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the stack lands |
| 2026-11-02 | 503 → 504 | +1 in PR #809 (`#session-tabs` 6/7): the stopped Agent tab is `DormantAgentTerminal` instead of `SessionActivitiesPane` (net 0), and the inspector drawer gets `activityClient` for its stopped-only Activity tab (+1). **Not split**: `#session-tabs` 7/7 (#810) wires `SessionTerminalTabs`' `sessionId` through this pane's strips, so the split is left to the stack's sweep node (`/pr-wrap` step 3.5, stack-branch rule) |
| 2026-11-07 | 504 → 504 | Moved with its package (#861, `#web-split` 17/20): `git mv` to `tddy-session-web/src/`, import specifiers only; no growth |


## What would close it

The selected-session wiring is three separable concerns, each a hook beside the component:

- the agent-conversation state and its layer (`useSessionAgentConversations`, `useConversationHosts`,
  the `SessionAgentConversationLayer` props and the per-runtime conversation props) — a
  `useSessionConversationWiring(selectedSession)` returning the props each strip spreads;
- the stopped strip's insert route (`dormantInsertRef`, `registerDormantInsert`,
  `insertPathIntoTerminal`) — a `useDormantInsertRoute(dormantStripShown, onInsertPathIntoTerminal)`;
- the base-view overlay choice (`customView` / stopped strip) — a `SessionBaseOverlay` component.

Hand split (TypeScript — the `code-restructuring` engine is Rust-only), green baseline first
(`DormantSessionTabsAcceptance`, `SessionAgentAttachTabAcceptance`,
`InactiveSessionActivitiesAcceptance`, `SessionFilesTabAcceptance` component specs), mechanical moves
only.

## Verified by hand

2026-11-02 — `wc -l` at the tip of PR #804 (503) and at `origin/master` `e9d036e0c` (468); at the wrap of PR #809 (504) against its base `feature/session-tabs/tab-plus` `0426e14e2` (503).
2026-11-03 — `wc -l` at `origin/master` `162a66978` (504) and at the wrap of #813 (504): **unchanged**, the sweep does not touch the file. Still open, deferred by the developer.
2026-11-07 — `wc -l` at the wrap of PR #842 (`#web-split` 5/16): 504 → 504. The PR only re-points hook imports to `tddy-client-web`; still open, not split (later `#web-split` nodes move this file).
2026-11-07 — `wc -l` at the wrap of PR #844 (`#web-split` 7/17): 504 → 504 against `bede09610`. The PR re-points `CreateSessionPane` to `tddy-session-create-web` (net 0). Still open, not split.
