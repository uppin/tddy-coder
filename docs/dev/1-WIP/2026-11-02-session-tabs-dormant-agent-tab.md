# Changeset: A stopped session's Agent tab shows its greyed history and offers Resume

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 3 — web only; a new `Toast` component, a read-only history terminal and an optimistic liveness hint threaded through the resume path
**Stack:** `#session-tabs 6/7` — branch `feature/session-tabs/dormant-agent-tab`, base `feature/session-tabs/tab-plus`
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
  instead of `SessionActivitiesPane`.
- The optimistic liveness hint (`useResumedSessions` in `src/components/sessions/useResumedSessions.ts`):
  - a successful `resumeSession` marks the session live until the next listing reports it;
  - `canResumeSession` / `connectionStatusForSession` callers in the pane read it.
  - The top-bar Resume and the toast share one resume action.

## Boundaries

- No daemon change: history comes from n2's existing RPC behaviour.
- No change to the tab strip host or the "+" (n1, n5).
- No global toast queue.
- The activities transcript component is unchanged, and still reachable from the activities view.

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

### Modified
- `SessionMainPane.tsx`, `useSessionsDrawerPaneActions.ts` (the shared resume action), `GhosttyTerminalSession.tsx` (exports only).
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/inactive-session-activities.md`.

## Testing Strategy

### Acceptance tests

**`tddy-web`**: `packages/tddy-web/cypress/component/DormantAgentTabAcceptance.cy.tsx`

- [ ] `a stopped session's Agent tab shows its recorded output in the history colour scheme`
- [ ] `typing into a stopped session's Agent tab sends no input`
- [ ] `a toast says the session is not running and offers Resume`
- [ ] `Resume from the toast goes live in the Agent tab without waiting for a session refresh`
- [ ] `a bash tab opened before Resume is the same terminal after it`
- [ ] `a failed Resume shows its error in the toast and keeps the greyed view`
- [ ] `a stopped session with no recorded output says so`

### Unit / integration tests

- [ ] `packages/tddy-web/cypress/component/Toast.cy.tsx`:
  - message and action render;
  - the action fires once;
  - busy disables it;
  - an error renders with `role="alert"`.
- [ ] `packages/tddy-web/src/components/sessions/useResumedSessions.test.ts` (bun):
  - marked live after a resume;
  - cleared when a listing reports the session live;
  - cleared when a listing reports it dormant again after the resume.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Agent tab and Resume | Cypress component, in-memory backend | The history RPC and `ResumeSession` are doubles |
| Liveness hint | bun unit | Pure state |

## Decisions & Trade-offs

- **Optimistic hint, not navigation:** the view stays derived from liveness (the inactive-activities
  design), and the hint is just a fresher liveness fact.
- **Terminal output over the transcript** in the Agent tab, as chosen in the interview.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-dormant-agent-tab-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
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
