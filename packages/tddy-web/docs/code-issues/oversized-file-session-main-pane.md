# oversized-file: SessionMainPane.tsx

**Location:** `packages/tddy-web/src/components/sessions/SessionMainPane.tsx`
**Category:** oversized-file
**Detected:** 2026-11-02 by `/pr-wrap` step 3.5 file-length gate on PR #804 (`#session-tabs` 1/7)
**Metrics:** **503 lines** (non-test source, whole file) · budget 500 · **1.006× over**
**Thresholds breached:** length 503 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-02 | 468 | `origin/master` (`e9d036e0c`), base of PR #804 |
| 2026-11-02 | 468 → 503 | +35 in PR #804: the stopped session's strip (`DormantSessionRuntime`) in place of the full-pane transcript, the agent-conversation layer and its per-session hosts, and the Files-tab Insert route to the stopped strip. **Not split**: `#session-tabs` 6/7 (#809) edits this same file, and a split here would turn its diff into a conflict (`/pr-wrap` step 3.5, stack-branch rule). Split after the stack lands |

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

2026-11-02 — `wc -l` at the tip of PR #804 (503) and at `origin/master` `e9d036e0c` (468).
