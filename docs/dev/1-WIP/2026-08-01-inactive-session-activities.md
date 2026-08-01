# Changeset: Inactive session shows its recorded activities, not the inspector

**Date**: 2026-08-01
**Status**: 🚧 In Progress
**Type**: Feature
**PRD**: [docs/ft/web/inactive-session-activities.md](../../ft/web/inactive-session-activities.md)
**Branch**: `feat-inactive-session-activities`

## Affected Packages

- **tddy-web**: [README.md](../../../packages/tddy-web/README.md)
  - [changesets.md](../../../packages/tddy-web/docs/changesets.md) — changeset index entry

No proto, daemon, or core change: `ConnectionService.ResumeSession` and
`ConnectionService.StreamAcpReplay` already exist and already serve a dormant session from persisted
`acp-transcript.jsonl` / `agent-activity.jsonl`. This is a web-only rewiring of which surface is shown
when.

## Related Feature Documentation

- [docs/ft/web/inactive-session-activities.md](../../ft/web/inactive-session-activities.md) — this feature
- [docs/ft/web/agent-activity-pane.md](../../ft/web/agent-activity-pane.md) — the transcript rendering this view reuses
- [docs/ft/web/session-drawer.md](../../ft/web/session-drawer.md) — Inspector drawer defaults change
- [docs/ft/web/url-state-routing.md](../../ft/web/url-state-routing.md) — no new params; `inspector` is no longer auto-written

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit tests
- [x] Implement feature
- [x] Update affected existing specs
- [x] Update package docs + changesets index

## Summary

Selecting an inactive session opens the Inspector over an empty pane whose only content is the text
"Select Resume to reconnect". The session's actual recorded history — the ACP transcript the daemon
persists and can replay without LiveKit — is reachable only through a top-bar popover that hides
itself when the pane is dormant enough to matter.

This inverts that. An inactive session's main pane becomes its **recorded activity transcript**, the
Inspector stays **closed** until asked for, and **Resume** moves into the pane's top bar where it is
present for every inactive session regardless of which base view is below it.

## Background

Three existing pieces already do the hard part and are simply not wired together:

- `StreamAcpReplay` replays from disk, so it serves an inactive session over the daemon-level client
  (`connection_service.rs:7785-7789`). No LiveKit room is required.
- `useAcpReplay` (`src/components/chat/useAcpReplay.ts`) already projects those frames into a
  read-only `AgentChatView` transcript with lazy-snapshot control.
- `ResumeSession` is already routed to the session's owning daemon by `handleResume`
  (`SessionsDrawerScreen.tsx:544-549`).

What is missing is a base view that renders the transcript, and a Resume affordance that does not
live inside the drawer we are about to stop opening.

## Scope

- [ ] Package Documentation
- [x] Implementation
- [x] Testing
- [x] Integration
- [ ] Technical Debt
- [x] Code Quality

## Technical Changes

### State A (Current)

- `inspectorState.ts:20-22` — `defaultInspectorOpen(isActive) { return !isActive }`: selecting an
  inactive session auto-opens the Inspector.
- `inspectorState.ts:26-28` — `isInspectorDocked(session)` is true for a disconnected session; the
  drawer then renders as the **full main pane** (`SessionInspectorDrawer.tsx:124`, `data-docked`).
- `SessionMainPane.tsx:298-344` — base view order is `customView` → runtime layer → connected
  placeholder → the literal string `"Select Resume to reconnect"` (line 341).
- `SessionMainPane.tsx:313` — `focused={!docked && r.sessionId === focusedRuntimeId}`: the focused
  runtime's foreground (and its claim-terminal overlay) is suppressed **because the inspector is
  docked**.
- `SessionMainPane.tsx:370-408` — top bar holds `AgentActivityOverlay`, `Add agent`, `Code`,
  `Inspector`. No Resume.
- `SessionMainPane.tsx:372-377` — `AgentActivityOverlay` is mounted for every session and self-hides
  when the transcript is empty (`AgentActivityOverlay.tsx:61-63`).
- `SessionsDrawerScreen.tsx:452-458` — on activation, writes the inspector default from
  `nextInspectorState(..., {type:"select", isActive})`.
- `SessionsDrawerScreen.tsx:522-527` — on `attachment.status === "idle"` for an inactive session,
  auto-opens the Inspector.
- Resume exists only at `SessionInspectorDrawer.tsx:270-281`
  (`sessions-inspector-resume-<sessionId>`), i.e. inside the drawer.

### State B (Target)

- The Inspector never opens on its own for a liveness reason — only an attach **error** opens it, and
  only a deep-linked `?inspector=<tab>` pre-opens it.
- The Inspector is always an overlay drawer; docking is gone.
- An inactive session whose base view would be the terminal shows the **Activities view**: the
  eagerly-loaded ACP transcript, full pane, with an explicit empty state.
- `PrStackScreen` / `WorkflowChatScreen` keep precedence — the Activities view replaces only the
  terminal base view.
- A `Resume` button sits in the top bar for every inactive session, calling the existing `onResume`.
- The `AgentActivityOverlay` icon is suppressed exactly when the Activities view is rendering.
- Runtime foreground suppression keys off **session inactivity**, not inspector docking.

### Delta

#### tddy-web — new

| File | Purpose |
|------|---------|
| `src/components/sessions/sessionBaseView.ts` | Pure `sessionBaseViewMode(session, hasWorkflowView) → "workflow" \| "activities" \| "terminal"` and `canResumeSession(session)`. Both derived from `connectionStatusForSession`, both unit-testable without React. |
| `src/components/sessions/SessionActivitiesPane.tsx` | The Activities view: `useAcpReplay` (eager `loadSnapshot` on mount) → `AgentChatView room={null} readOnly`, tool-detail dialog, empty state. `data-testid="sessions-activities-pane"`. |
| `cypress/support/pages/sessionActivitiesPage.ts` | Page object for the new surface — all raw selectors live here. |
| `cypress/component/InactiveSessionActivitiesAcceptance.cy.tsx` | Acceptance specs (below). |
| `src/components/sessions/sessionBaseView.test.ts` | Unit tests for the two pure functions. |

#### tddy-web — modified

| File | Change |
|------|--------|
| `src/components/sessions/inspectorState.ts` | `defaultInspectorOpen()` returns `false` unconditionally (drops its `isActive` parameter); delete `isInspectorDocked`. |
| `src/components/sessions/SessionMainPane.tsx` | Base view switches on `sessionBaseViewMode`; Activities view renders over the still-mounted runtime layer; `focused` keyed on inactivity; top-bar `Resume`; suppress the activity overlay icon when the Activities view shows; drop `docked`. |
| `src/components/sessions/SessionInspectorDrawer.tsx` | Remove the `docked` prop, the `data-docked` attribute, and the full-pane class at line 124. |
| `src/components/sessions/SessionsDrawerScreen.tsx` | Activation writes `"closed"` (no liveness branch); delete the idle+inactive auto-open at 522-527, keep the `error` branch; delete the now-vacuous `inspectorAutoOpenRef`. Plus the liveness-attach effect below. |
| `src/components/sessions/inspectorState.test.ts` | Rewrite the `defaultInspectorOpen` / `isInspectorDocked` / `select`-action cases for the new contract. |
| `cypress/support/testIds.ts` | Add `sessionsActivitiesPane`, `sessionsActivitiesEmpty`, `sessionsMainResumeBtn(sessionId)`. |

#### tddy-web — existing specs that assume the old defaults

Each selects an inactive session and expects the Inspector already open, or asserts `data-docked`.
They need an explicit `inspectorToggle().click()` (or the docking assertion dropped) — the behavior
they cover is otherwise unchanged.

| Spec | Fix |
|------|-----|
| `cypress/component/SessionInspectorDockedDisconnected.cy.tsx` | Rewrite: the inspector does **not** dock; claim-terminal stays suppressed for an inactive session (now via inactivity). Filename becomes stale — rename pending consent. |
| `cypress/component/SessionInspectorAcceptance.cy.tsx` | Add explicit toggle where auto-open was assumed (≈ lines 130, 145, 169). |
| `cypress/component/SessionInspectorUsageAcceptance.cy.tsx` | Explicit toggle before line 78. |
| `cypress/component/SessionInspectorVncAcceptance.cy.tsx` | Explicit toggle before line 58. |
| `cypress/component/SessionInspectorScreenSharingAcceptance.cy.tsx` | Explicit toggle before line 58. |
| `cypress/component/PrStackInspectorAcceptance.cy.tsx` | Explicit toggle; drop the "which auto-opens the inspector" comment at line 71. |
| `cypress/component/SessionsDrawerAcceptance.cy.tsx` | Explicit toggle before line 269. |
| `cypress/component/SessionInspectorUrlStateAcceptance.cy.tsx` | Found by the full-suite run, not by the initial scope pass: 3 tests reach the inspector through `appLocationPage` rather than `inspectorDrawer()`, so the auto-open grep missed them. Explicit toggle added. |
| `cypress/component/PrStackViewRoutingAcceptance.cy.tsx` | Also missed by the scope pass — it asserted the literal `"Select Resume to reconnect"` text for an inactive claude-cli session. Now asserts the Activities pane; the test's actual subject ("neither workflow view renders") is unchanged. |

`SessionInspectorAcceptance`'s attachment-driven describe covered two behaviours this changeset deletes
(auto-close on connect, auto-open on idle). They were inverted into their new-contract counterparts
rather than deleted, so the setups keep earning their keep.

## Implementation Milestones

- [ ] `sessionBaseView.ts` + its unit tests
- [ ] `SessionActivitiesPane` + page object + test ids
- [ ] `SessionMainPane` base-view switch, top-bar Resume, overlay suppression, runtime focus rekey
- [ ] Inspector default closed + docking removal (`inspectorState`, `SessionInspectorDrawer`, `SessionsDrawerScreen`)
- [ ] Update the seven affected existing specs
- [ ] Package docs + `packages/tddy-web/docs/changesets.md` entry

## Testing Plan

### Testing Strategy

**Primary: Cypress component tests over `anInMemoryRpcBackend`.** Every behavior in this changeset is
a rendering decision driven by RPC-shaped inputs (`SessionEntry.is_active`, `StreamAcpReplay` frames)
and observed as DOM. The in-memory backend already models the two-phase replay protocol
(`cypress/support/rpc/acpReplay.ts`) and records unary calls, so "clicking Resume calls ResumeSession
for this session id" is directly assertable — no `cy.intercept`, no live daemon.

Two mounting levels, matching the precedent in `SessionInspectorDockedDisconnected.cy.tsx`:
- **Full `SessionsDrawerScreen`** over `mountWithRecordingLiveKitRpc` for anything involving
  selection, inspector defaults, or URL state.
- **`SessionMainPane` direct** for base-view selection with an explicit `runtimes` array — the only
  way to pin "a mounted runtime stays behind the Activities view, unfocused".

**Secondary: `bun:test` unit tests** for the two pure functions that encode the view rule
(`sessionBaseViewMode`, `canResumeSession`) and the amended `inspectorState`. These are where the
decision table is pinned exhaustively, so the Cypress specs can stay behavioral rather than
combinatorial.

### Coverage Requirements

- [x] Happy path — inactive session shows activities + Resume; active session shows terminal
- [x] Error scenarios — empty transcript; transcript stream failure leaves partial content
- [x] Edge cases — pr-stack / workflow-chat precedence; `needs-input` counts as active; no session
- [x] Integration points — `ResumeSession` reaches the owning daemon; `?inspector=` deep link still wins
- [x] Regression — inspector stays closed on selection; claim-terminal still suppressed; runtimes stay mounted

## Acceptance Tests

### tddy-web — `cypress/component/InactiveSessionActivitiesAcceptance.cy.tsx`

- [ ] **Cypress**: `shows the recorded activity transcript as the main view when an inactive session is selected` — the Activities pane renders and carries the replayed entries, in place of the "Select Resume to reconnect" placeholder
- [ ] **Cypress**: `keeps the inspector closed when an inactive session is selected` — `data-state="closed"` with no toggle click (the core inversion)
- [ ] **Cypress**: `offers Resume in the pane top bar for an inactive session` — top-bar button present without opening the inspector
- [ ] **Cypress**: `resumes the session through the owning daemon when the top-bar Resume is clicked` — `ResumeSession` recorded exactly once, carrying that session id
- [ ] **Cypress**: `shows an explicit empty state when the inactive session recorded no activity` — empty-state marker present, transcript absent, Resume still offered
- [ ] **Cypress**: `shows the terminal and no Resume button for an active session` — the inverse case stays untouched
- [ ] **Cypress**: `keeps the planned-PR view for an inactive pr-stack session and still offers Resume` — workflow precedence plus universal Resume, in one behavior
- [ ] **Cypress**: `keeps the workflow chat view for an inactive workflow session` — `WorkflowChatScreen` is not replaced
- [ ] **Cypress**: `hides the agent activity overlay icon while the activities view is showing` — no duplicate transcript surface
- [ ] **Cypress**: `keeps the agent activity overlay icon for an inactive pr-stack session` — suppression is scoped to the Activities view, not to inactivity
- [ ] **Cypress**: `opens the tool call detail dialog from an entry in the activities view` — `GetAcpToolCallDetail` wiring survives the new mount
- [ ] **Cypress**: `honours a deep-linked inspector tab for an inactive session` — `?inspector=files` still pre-opens the drawer
- [ ] **Cypress**: `keeps a mounted runtime unfocused behind the activities view` — runtime layer present, focused-terminal marker and claim-terminal overlay absent
- [ ] **Cypress**: `returns to the terminal view once the session becomes active` — liveness flip swaps the base view back and drops Resume

### tddy-web — rewritten `cypress/component/SessionInspectorDockedDisconnected.cy.tsx`

- [ ] **Cypress**: `renders the inspector as an overlay drawer for a disconnected session, not as the main pane` — replaces the docking AC
- [ ] **Cypress**: `keeps the inspector an overlay drawer for a connected session` — unchanged behavior, docking assertion dropped

## Unit Tests

### tddy-web — `src/components/sessions/sessionBaseView.test.ts`

- [ ] `sessionBaseViewMode` returns `"activities"` for an inactive session with no workflow view
- [ ] `sessionBaseViewMode` returns `"terminal"` for an active session
- [ ] `sessionBaseViewMode` returns `"workflow"` for an inactive session that has a workflow view
- [ ] `sessionBaseViewMode` returns `"workflow"` for an active session that has a workflow view
- [ ] `sessionBaseViewMode` returns `"terminal"` for a needs-input session (pending elicitation reads as active)
- [ ] `sessionBaseViewMode` returns `"terminal"` when there is no session
- [ ] `canResumeSession` is true for an inactive session
- [ ] `canResumeSession` is false for an active session
- [ ] `canResumeSession` is false for a needs-input session
- [ ] `canResumeSession` is false when there is no session

### tddy-web — `src/components/sessions/inspectorState.test.ts` (amended)

- [ ] `defaultInspectorOpen` returns false for an active session
- [ ] `defaultInspectorOpen` returns false for an inactive session (the changed contract)
- [ ] `select` action closes the drawer for an inactive session
- [ ] `select` action closes the drawer for an active session
- [ ] existing `open`/`close`/`toggle`/`expand`/`restore` cases unchanged
- [ ] `isInspectorDocked` cases removed with the function

## Decisions & Trade-offs

**The Activities view replaces only the terminal base view.** `PrStackScreen` renders a planned-PR
control surface from persisted stack state and `WorkflowChatScreen` is already a transcript; both stay
meaningful when dormant. Replacing them with a second transcript would remove working functionality.
Rejected: a literal "every inactive session shows activities", which would strand an inactive pr-stack
orchestrator's planned-PR list behind a resume.

**Resume is universal even where the base view is not.** The button is keyed on liveness alone, so it
occupies the same top-bar position for a dormant pr-stack session as for a dormant terminal session.
Rejected: tying Resume to the Activities view, which would leave workflow sessions with no resume
affordance once the Inspector stops auto-opening.

**Docking is removed rather than retained.** Docking existed only because the pane behind the drawer
was empty for a disconnected session. It no longer is, and a full-pane drawer would bury the very
transcript this changeset surfaces. Cost: `data-docked` and its assertions disappear from seven specs.
Rejected: keeping docking, which makes the manual `Inspector` toggle destroy the default view.

**Runtime focus suppression is rekeyed from docking to inactivity.** `focused={!docked && …}` happened
to produce the right result only because docking and disconnection were the same predicate. Naming the
real cause keeps the claim-terminal overlay suppressed once docking is gone.

**The snapshot loads eagerly here, lazily in the overlay.** The overlay defers because it is a popover
the operator may never open; the Activities view *is* the view, so deferring would only add a blank
frame. Both paths share `useAcpReplay` and the same registry cache, so a session read in one surface
costs nothing in the other.

**A resumed session needed a second attach effect, not just a view rule.** The activation effect
issues `ConnectSession` only for a session that is live *at selection time*, so a session resumed
from the new top-bar button stayed on its Activities view until it was re-selected — the view rule
alone could not deliver "returns to the terminal once active". `SessionsDrawerScreen` gained an
effect that attaches a selected session which comes alive, guarded by `attachRequestedSessionIdRef`
(written by both effects) so the two can never both fire `ConnectSession` for one session. Rejected:
having the Resume handler attach directly, which would have covered the button but not a session
resumed from another screen and observed on the next list poll.

**No new URL parameter.** The Activities view is derived from liveness, which the web already computes
from common-room participants. It is not a navigable selection, so per `url-state-routing.md` it stays
out of the address bar — and a resumed session's view corrects itself without any state to migrate.

## Validation Results

Four analyses over the branch diff: change risk, test quality, production readiness, clean code. No
Rust package is touched, so the `cargo` gates are N/A — the gates here are `bun test`, the Cypress
component suite, and `vite build`.

### Defects found and fixed

1. **`attachRequestedSessionIdRef` stranded a re-resumed session.** The ref was never cleared when a
   session went dormant, so a second Resume within one selection never re-attached
   (dormant→live→dormant→live), and a stale `connected-*` registry entry let the fast path pre-claim
   the ref for a dormant session, blocking even the first Resume. The ref now means "an attach has
   been taken for the current *live epoch*". Pinned by a new re-resume test.
2. **Every Resume issued a duplicate `ConnectSession`.** `resumeSession` already drives the
   attachment to `connected-livekit`; the liveness effect then fired again on the next poll, minting
   a fresh browser identity and forcing a terminal reconnect plus a second `ClaimTerminalControl`.
   Pinned by a `callsTo(connectSession)` assertion.
3. **The Activities view claimed "This session recorded no activity" before the count feed
   answered — and identically when it failed.** `hasActivity` is `count > 0` starting from 0, so
   every dormant session flashed a false statement and a failed replay was indistinguishable from an
   empty one (which PRD § Edge cases forbids). The registry now tracks `countLoaded`, set on the
   first count frame whatever its value; the empty state renders only once the count is known to be
   zero.
4. **Two of the inverted inspector tests were tautologies** — precondition and outcome asserted the
   same thing, so they could not fail. Each now proves the state actually changed first.
5. **The docked-vs-overlay tests could no longer detect docking.** `data-docked` was their only
   discriminator and it was deleted; both replacement assertions had also been true in docked mode.
   They now assert the overlay's rendered footprint.

### Deliberate cross-cutting change

**`connectionStatusForSession` no longer reports `needs-input` for a dead session.** A session with
`is_active = false` and a stale `pending_elicitation` used to read as live, which under this
changeset's rules left it with no Activities view *and* no Resume button — a dead end reachable when
an agent dies mid-elicitation. The predicate is now `is_active && pending_elicitation`.

This reaches beyond the feature: it also changes the drawer status dot
(`SessionDrawerItem.tsx`, `SessionDrawer.tsx`), the live/remaining partition
(`sessionStackGroups.ts`), and session sorting. A dead session with a stale elicitation flag now
shows as disconnected everywhere, which is what it is. Approved explicitly rather than worked around
locally, because the alternative — a second liveness predicate for Resume alone — would have left
two rules disagreeing about the same session.

### Known gaps

- **The removal of docking is not pinned by a test.** `data-docked` was the only discriminator the
  component harness could observe, and the harness loads no stylesheet — every Tailwind width is
  inert, so a geometry assertion fails with `expected 1280 to be below 1280` regardless of the
  layout. The renamed spec pins the adjacent fact (the base view stays mounted behind an open
  drawer, which the docked layout used to replace). Written up in `docs/dev/TODO.md`.
- **Resume has up to ~2s of dead time** before the pane swaps, because liveness only refreshes on the
  drawer's 2s `ListSessions` poll. Falls directly out of "the view is derived from liveness"; noted
  in `docs/dev/TODO.md` with a suggested optimistic-hint fix.

### Accepted, not fixed

- `SessionActivitiesPane` and `AgentActivityOverlay` share ~25 lines (resolved client, replay hook,
  detail-dialog state). Extractable as a `useAcpReplaySurface` hook; left alone because the two
  surfaces may legitimately diverge and neither is complex today.
- `SessionsDrawerScreen` remains a ~790-line screen component. Unchanged in kind by this diff;
  splitting it is its own changeset.
- Cypress fixtures use `as unknown as SessionEntry` rather than `create(SessionEntrySchema, …)`,
  matching existing style in the specs they sit beside.

## Technical Debt & Production Readiness

- `SessionDetailPane.tsx` carries a second, unreferenced Resume button (`sessions-detail-resume-*`)
  and has no importers. Out of scope here; flagged for `docs/dev/TODO.md`.
- `useSessionActivity.ts` (the `StreamSessionActivity` consumer) remains callerless — the overlay and
  this view both use the ACP replay path. Out of scope; flagged for `docs/dev/TODO.md`.

## References

- PRD: [docs/ft/web/inactive-session-activities.md](../../ft/web/inactive-session-activities.md)
- Replay host: `packages/tddy-daemon/src/connection_service.rs:7785-7789`
- Replay test helpers: `packages/tddy-web/cypress/support/rpc/acpReplay.ts`
