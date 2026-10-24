# Changeset: The create pane shows each step and a live clone bar

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — one package (`tddy-web`), a hook state change plus a new component, Cypress component tests over the in-memory RPC backend
**Stack**: #start-progress 7/8. PR: #740. Base: `feature/start-progress/supervised-clone-progress`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-progress-ui-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

No `docs/dev/todo/` entry or `tddy-web` code issue is in this node's path (the three open `tddy-web`
issues are the ghostty terminal files and `use-model-registry-fan-out`).

## Responsibility

Rendering a streamed start's progress in the create pane. Owns:

- `useSessionAttachments` (or a new `useStartProgress` it composes — see Decisions): state for the
  ordered steps a start has reported (`pending | active | done`), the latest `CloneProgress`, and the
  step a failed stream stopped at.
- `CreateSessionStartProgress` (replaces `CreateSessionStartPhase`): heading "Creating session on
  <host label>", step rows (Cloning repository · Worktree · Semantic index · Agent, each shown only when
  it applies), the clone row's stage, percent bar, `bytes · throughput` (binary units).
- A `formatBytes` / `formatThroughput` helper if none exists.
- Page-object helpers in `cypress/support/pages/createSessionPage.ts`; the stale comment in
  `SessionStartAndIndexingProgress.cy.tsx` ("streams a start only when something is attached").

## Boundaries

- Does not add Cancel (node 8) or touch `CreateSessionActions`.
- Does not change the request, the transport, attachment progress rows or the passphrase prompt.
- No daemon changes; the events come from nodes 4–6.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/start-progress/clone-progress` (5/8) | `StartPhase_Step.CLONE`, `CloneProgress`, `StartSessionEvent.clone_progress` in `src/gen/session_pb.ts` | renders them | regenerate or edit `session_pb.ts` |
| `feature/start-progress/split-stream` (4/8) | the codebase host's steps relayed on a split start | nothing in code — a split start's events look like a local one's | add split-specific UI |

## Draft PR contract

```ts
// src/components/sessions/CreateSessionStartProgress.tsx (new; CreateSessionStartPhase.tsx removed)
export interface StartProgressState {
  steps: { step: StartPhase_Step; state: "pending" | "active" | "done" }[];
  clone: CloneProgress | null;
  stoppedAt: StartPhase_Step | null;
}
export function CreateSessionStartProgress(props: { hostLabel: string; progress: StartProgressState }): JSX.Element | null;
// bytes and throughput render through the existing formatAttachmentBytes (binary units)
// src/components/sessions/startProgress.ts (new): StartProgressState, NO_START_PROGRESS, startProgressAfter(state, event), startProgressFailed(state)
// useSessionAttachments → returns `startProgress: StartProgressState` instead of `startPhase` (green)
// data-testids: create-session-start-progress, create-session-start-step-<name>, create-session-clone-progress
```

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — its tests render `StartPhase_Step.CLONE` and `CloneProgress` from node
5's regenerated `session_pb.ts`. Those types arrive with node 5's contract commit, but the pipeline waits
for node 5 to be green so this node is not rebased under a moving proto.
**Concurrent with:** supervised-clone-progress (6/8)
**Blocks:** none — start-cancel (8/8) adds its own button to the same pane and tests only that.

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — create pane start progress

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-progress-ui.md](../../ft/web/1-WIP/PRD-2026-10-24-start-progress-progress-ui.md)
- [session-drawer.md](../../ft/web/session-drawer.md) § Start progress — rewritten at wrap

## Summary

The create pane renders a streamed start as a step list with a live clone progress row.

## Background

Today one `<p>` names the step in progress ("Creating worktree…") and disappears between steps; the
clone has no step and no progress; the operator on the 2026-10-24 incident watched a disabled button
for 30 s.

## Scope

- [ ] Start progress state in the hook (steps, clone, stopped-at)
- [ ] `CreateSessionStartProgress` component; `CreateSessionStartPhase` removed
- [ ] Clone row: stage, percent bar, bytes and throughput
- [ ] Byte / throughput formatting — reuses `formatAttachmentBytes` (`src/lib/attachmentBytes.ts`, binary units) instead of a new helper
- [ ] Page object + stale spec comment
- [ ] Docs: `session-drawer.md` § Start progress

## Technical Changes

### State A
`startPhase: StartPhase_Step | null` set on BEGIN, cleared on matching END and in `finally`;
`CreateSessionStartPhase` renders one line from `STEP_TEXT`.

### State B
`startProgress` accumulates reported steps in report order with their state; `clone_progress` events
replace `clone`; on stream error the active step becomes `stoppedAt`; the component renders the list
under the host heading and the clone row while the clone step is active.

## Testing Plan

Cypress component tests with `mountWithRpc` + `anInMemoryRpcBackend`, a gated `streamStartSession`
generator (the `aGate` technique) so each mid-stream state is asserted exactly; fluent page-object
assertions. Single-spec runs only.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `lists the steps of the start in order, ticking each as it ends` | `packages/tddy-web/cypress/component/CreateSessionStartProgress.cy.tsx` (new) | step list |
| `shows the clone's stage, percent, bytes and throughput, updating in place` | same | clone row |
| `shows no clone row for a start that does not clone` | same | conditional row |
| `shows the step a failed start stopped at, with the error` | same | failure |
| `names the host doing the work` | same | heading (codebase host on a split start) |

`SessionStartAndIndexingProgress.cy.tsx`'s create-pane case is updated to the new page object.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **A separate `useStartProgress` reducer composed by `useSessionAttachments`** is preferred over
  growing the attachments hook further; decided in pass 2 by the hook's size.
- **Steps appear as reported**, not from a fixed list: the host decides whether a start clones or
  indexes.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-start-progress-progress-ui-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer for the whole of pass 2, 2026-10-24)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the changed specs only (`cypress run --component --spec …`); full Cypress from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
