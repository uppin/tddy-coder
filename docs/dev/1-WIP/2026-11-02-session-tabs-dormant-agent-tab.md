# Changeset: A stopped session's Agent tab shows its greyed history and offers Resume

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 3 — web only; a new `Toast` component, a read-only history terminal and an optimistic liveness hint threaded through the resume path
**Stack:** `#session-tabs 6/7` — branch `feature/session-tabs/dormant-agent-tab`, base `feature/session-tabs/tab-plus` · **PR:** [#809](https://github.com/uppin/tddy-coder/pull/809)
**PRD:** [PRD-2026-11-02-dormant-agent-tab.md](../../ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md)

## Initial Discovery

[2026-11-02-session-tabs-dormant-agent-tab-initial-discovery.md](./2026-11-02-session-tabs-dormant-agent-tab-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-08-01-tddy-web-inactive-session-activities-follow-ups.md](../todo/2026-08-01-tddy-web-inactive-session-activities-follow-ups.md) | ✅ **Partly RESOLVED HERE**: the bullet "Resume has up to a ~2s dead time before the view changes" is closed by the optimistic liveness hint on a successful `ResumeSession`. The CSS-less-harness bullet stays, so at wrap the record is **narrowed**, not deleted |
| `packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal-session.md` | ⚠ During — the greyed terminal is a new component (`DormantAgentTerminal.tsx`); the theme constants it shares are exported, not duplicated, and `GhosttyTerminalSession.tsx` gains no logic |

## Affected Packages

- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): `Toast`, `DormantAgentTerminal`, the optimistic liveness hint

## Responsibility

- `src/components/ui/toast.tsx`: `Toast({ message, action?: { label, onClick, busy? }, error?, testId })`, with `role="status"`, an absolutely positioned card styled like `TerminalControlOverlay`.
- `src/components/sessions/DormantAgentTerminal.tsx`: `DormantAgentTerminal({ sessionId, sessionToken, terminalClient, onResume })`.
  - A read-only `GhosttyTerminal` (no `onData` sink), filled with `createForwardHistoryFetcher` for
    terminal `main` up to `at_end`.
  - It uses `HISTORY_THEME` and `HISTORY_FILTER`, exported from `GhosttyTerminalSession.tsx`.
  - It shows "No recorded output" (`dormant-agent-terminal-empty`) on `not_found`.
  - It shows the `Toast` "This session is not running", with Resume.
- `SessionMainPane` fills n1's `DormantSessionRuntime` `agentTab` slot with `DormantAgentTerminal`
  instead of `SessionActivitiesPane` (+1 line there: `activityClient` to the inspector drawer).
- The inspector's **Activity** tab, for a stopped session only (developer decision, 2026-11-02):
  - `src/routing/appRoutes.ts`: `activity` joins `INSPECTOR_TAB_NAMES`, and `DORMANT_INSPECTOR_TAB_NAMES`
    names it as the stopped-only tab.
  - `src/components/sessions/InspectorTabs.tsx`: an `activityAvailable` prop shows the "Activity" button
    (`sessions-inspector-tab-activity`).
  - `src/components/sessions/SessionInspectorActivityTab.tsx` (new): renders `SessionActivitiesPane`
    (the recorded transcript, with its empty state) for the session.
  - `SessionInspectorDrawer.tsx`: `activityAvailable = canResumeSession(session)`. A live session's
    `?inspector=activity` degrades to Details, as an unavailable media tab does.
- The optimistic liveness hint (`src/components/sessions/useResumedSessions.ts`). It is pure state —
  `markResumed(resumed, id)`, `afterListing(resumed, listed)`, `isLive(resumed, session)` over a
  `ReadonlySet<string>` — which the pane holds and threads through:
  - a successful `resumeSession` marks the session live until the next listing reports it;
  - `canResumeSession` / `connectionStatusForSession` callers in the pane read it.
  - The top-bar Resume and the toast share one resume action.

## Boundaries

- No daemon change: history comes from n2's existing RPC behaviour.
- No change to the tab strip host or the "+" (n1, n5). The terminal tab strip is unchanged; only the
  inspector's tab strip gains its stopped-only Activity tab.
- No global toast queue.
- `SessionActivitiesPane` (the transcript component) is unchanged. It is mounted again, by the inspector's
  Activity tab of a stopped session, and nowhere else.
- The docked Agent Activity sidebar stays suppressed in the activities mode (one transcript per pane).
  The Activity tab is the stopped session's transcript route, not a lifted sidebar suppression.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` dormant-tabs | `DormantSessionRuntime` with its `agentTab` slot; dormant sessions render a strip | fills the slot | change the strip host, its props or the roster |
| `n2` agent-history | `GetTerminalHistory(main)` answers for a stopped session | `DormantAgentTerminal` reads it (a double in tests) | touch recording, the port or the proto |

## Draft PR contract

The first push publishes:

- `Toast`, `DormantAgentTerminal` and `useResumedSessions` with their props and signatures, rendering
  nothing or returning the identity (`// TODO(dormant-agent-tab): implement`);
- the `HISTORY_*` exports;
- the failing tests below.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** after n1 is green — its acceptance tests select a stopped session and need n1's strip to render the Agent tab. n2 is a double here (in-memory `GetTerminalHistory`), so n2's behaviour is not needed to green it, only to make it useful in production
**Concurrent with:** n7 `tmux-new`
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    n1 → n6      n2 → n6      n3 → n7

## State A (after n1, n2)

- A stopped session's Agent tab shows the transcript.
- Resume lives in the top bar, and the view waits for the 2 s poll.
- No toast component exists.

## State B

- The Agent tab is a greyed read-only terminal of the last recorded output, with a Resume toast.
- Resume goes live in place at once; the other tabs are untouched.

## Delta

### New
- `packages/tddy-web/src/components/ui/toast.tsx`
- `packages/tddy-web/src/components/sessions/DormantAgentTerminal.tsx`
- `packages/tddy-web/src/components/sessions/useResumedSessions.ts`
- `packages/tddy-web/src/components/sessions/SessionInspectorActivityTab.tsx` (the inspector's Activity tab, stopped sessions only)

### Modified
- `SessionMainPane.tsx`, `useSessionsDrawerPaneActions.ts` (the shared resume action), `GhosttyTerminalSession.tsx` (exports only).
- `src/routing/appRoutes.ts` (`activity`, `DORMANT_INSPECTOR_TAB_NAMES`), `src/components/sessions/InspectorTabs.tsx` and `SessionInspectorDrawer.tsx` (the Activity tab).
- Cypress: `cypress/support/pages/sessionsDrawerPage.ts` (`inspectorActivityTab`), `cypress/support/testIds.ts`; the `onResumeSession` prop on the mount sites of the SessionMainPane specs.
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/inactive-session-activities.md`.

## Testing Strategy

### Acceptance tests

**`tddy-web`**: `packages/tddy-web/cypress/component/DormantAgentTabAcceptance.cy.tsx`

- [x] `a stopped session's Agent tab shows its recorded output in the history colour scheme`
- [x] `typing into a stopped session's Agent tab sends no input`
- [x] `a toast says the session is not running and offers Resume`
- [x] `Resume from the toast goes live in the Agent tab without waiting for a session refresh`
- [x] `a bash tab opened before Resume is the same terminal after it`
- [x] `a failed Resume shows its error in the toast and keeps the greyed view`
- [x] `a stopped session with no recorded output says so`
- [x] `a stopped session's recorded transcript is in the inspector's Activity tab` (added 2026-11-02, decision 2)

The n1 specs that pinned the transcript in the Agent tab were rewritten to State B, not deleted
(decision 1, 2026-11-02). Each keeps its intent:

- `DormantSessionTabsAcceptance`: "a stopped session shows its tab strip with the greyed recorded output in the Agent tab";
- `InactiveSessionActivitiesAcceptance` (8): the main view is `dormant-agent-terminal`; the transcript
  content and the empty state are asserted on the inspector's Activity tab; the return-to-terminal and
  re-attach cases watch the Agent tab; "one transcript per pane" keeps the overlay icon hidden while the
  Activity tab shows the transcript; the runtime case stays unfocused behind the Agent tab;
- `SessionInactiveInspectorOverlay`: "keeps a disconnected session's Agent tab mounted behind the open inspector";
- `PrStackViewRoutingAcceptance`, `SessionInspectorAcceptance`: the dormant base view is the Agent tab.

### Unit / integration tests

- [x] `packages/tddy-web/cypress/component/Toast.cy.tsx`:
  - message and action render;
  - the action fires once;
  - busy disables it;
  - an error renders with `role="alert"`.
- [x] `packages/tddy-web/src/components/sessions/useResumedSessions.test.ts` (bun):
  - `a stopped session is live as soon as it is resumed`;
  - `a resumed session stays live while the listing still reports it stopped`;
  - `once the listing reports it live, the listing alone decides again` (a guard);
  - `a session nobody resumed is as live as the listing says` (a guard).
- The in-memory terminal fake gains `terminalHistoryNotFound` (a `NOT_FOUND` history) and `atEnd`
  on scripted chunks.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Agent tab and Resume | Cypress component, in-memory backend | The history RPC and `ResumeSession` are doubles |
| Liveness hint | bun unit | Pure state |

## Decisions & Trade-offs

- **Optimistic hint, not navigation:** the view stays derived from liveness (the inactive-activities
  design), and the hint is just a fresher liveness fact.
- **Terminal output over the transcript** in the Agent tab, as chosen in the interview.
- **Decision 1 (developer, 2026-11-02): the n1 assertions move to State B, none deleted.** The Agent tab
  expects `DormantAgentTerminal` (`dormant-agent-terminal`, history scheme); each rewritten case keeps its
  intent (see Acceptance tests). The Cypress mount sites that spread an untyped handler set or mount
  `SessionMainPane` inline (11 spec files) now pass `onResumeSession`; the shared noop objects carry it.
- **Decision 2 (developer, 2026-11-02): the recorded transcript lives in the inspector's Activity tab.**
  There was no Activity tab in the inspector before this change, so the approved route is a new one:
  `activity` in `INSPECTOR_TAB_NAMES`, offered only for a stopped session (`DORMANT_INSPECTOR_TAB_NAMES`),
  rendering the existing `SessionActivitiesPane`. The developer's note named the suppression to lift, but
  the code suppresses the docked Agent Activity sidebar in the activities mode, and lifting it would show
  `AgentChatView` rather than `SessionActivitiesPane`. The sidebar therefore stays suppressed, which keeps
  one transcript per pane. A stopped PR-Stack or workflow session also gets the Activity tab, because the
  rule is "a stopped session" and not "a stopped session in the activities mode".
- **Decision 3 (developer, 2026-11-02), accepted deviation:** `DormantAgentTerminal` passes
  `preventFocusOnTap={isMobile}` to `GhosttyTerminal`. On a touch device a tap does not focus the greyed
  terminal, so the keyboard stays down over a view that takes no input. A pointer device keeps the default
  focus behaviour. The view still sends no input (the spec "typing into a stopped session's Agent tab sends
  no input" is green).

## Baseline before green

Recorded at the contract commit (2026-11-02), scoped to `tddy-web`:

- **This node's red tests:**
  - `DormantAgentTabAcceptance.cy.tsx`: 7 failing. Six fail on the missing `dormant-agent-*` elements. `a bash tab opened before Resume…` fails earlier, on n1's missing stopped-session "+", which is the recorded n1 → n6 dependency.
  - `Toast.cy.tsx`: 4 failing.
  - `useResumedSessions.test.ts`: 2 failing (+ 2 guards passing on the identity stub).
- `HISTORY_THEME` / `HISTORY_FILTER` are now exported from `GhosttyTerminalSession.tsx`; no logic changed there.

Green, scoped to `tddy-web` (2026-11-02, after decisions 1 and 2; the full Cypress suite was not run):

- `bun test src/components/sessions src/routing`: 555 pass, 0 fail.
- Cypress component, all green: `DormantAgentTabAcceptance` 8/8, `Toast` 4/4, `DormantSessionTabsAcceptance` 11/11,
  `InactiveSessionActivitiesAcceptance` 18/18, `SessionInactiveInspectorOverlay` 6/6,
  `SessionInspectorInactiveDaemonSourced` 1/1, `SessionAgentAttachTabAcceptance` 14/14, `TerminalHistoryPaging` 27/27,
  `WorkflowChatResumeAcceptance` 2/2, `SessionStartResumeStillDaemonRouted` 3/3; and the specs whose mount sites
  gained `onResumeSession` or whose assertions moved (`SessionInspectorAcceptance`, `PrStackViewRoutingAcceptance`,
  `SessionInspectorUrlStateAcceptance`, `SessionInspectorVncAcceptance`, `SessionTerminalFeedRecoveryAcceptance`,
  `SessionInspectorMediaCapabilityAcceptance` and the SessionMainPane / PrStack / GhosttyTerminalShrink mount specs), 0 failing.
- `tsc`: no `onResumeSession` error remains; the changed source files have no new error. The project's pre-existing
  tsc errors (718 in total) are not this change's.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-dormant-agent-tab-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-11-02 (reviewed with the stack at the end of pass 2)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /green (TDD green phase)
- **Resolved (2026-11-02, decisions 1 and 2):** the stopped session's Agent tab holds `DormantAgentTerminal`,
  and the recorded transcript is the inspector's Activity tab. The nine n1 assertions that pinned State A are
  rewritten (see Acceptance tests), and `SessionInactiveInspectorOverlay`, `PrStackViewRoutingAcceptance` and
  `SessionInspectorAcceptance` got the same treatment when the scoped run found them.
- **Resolved:** `SessionMainPane` takes `onResumeSession`, and the Cypress mount sites pass it.
- **Open for the wrap, not for green:** `docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md` and the
  inspector docs (`docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/inactive-session-activities.md`) should
  describe the Activity tab at wrap. The PRD is not edited in this stage.
- `SessionMainPane` takes a new required prop, `onResumeSession`. Fifteen Cypress component mount sites that spread an untyped handler set no longer type-check (they run unchanged); add the prop when those specs are reconciled.
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
