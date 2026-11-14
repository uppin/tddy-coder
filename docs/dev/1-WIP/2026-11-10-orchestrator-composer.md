# Changeset: start an orchestrator from the project entry

**Date**: 2026-11-10
**Status**: 🚧 In Progress — draft-PR contract (commit 2): surface stubbed, failing tests written
**Type**: Feature
**Complexity**: 3 — web only, but three packages (session-web, session-create-web, client-web route) and a new screen reusing extracted form pieces
**Stack**: `#orchestrators` 8/11 · branch `feature/orchestrators/orchestrator-composer` · PR #885 · base `feature/orchestrators/stack-plan-tools` (#884)

## Initial Discovery

[2026-11-10-orchestrator-composer-initial-discovery.md](./2026-11-10-orchestrator-composer-initial-discovery.md)

## Prerequisites

### ✅ RESOLVED HERE — SessionDrawer.tsx grew past 500 lines — [`2026-10-23-session-drawer-grew-past-500-by-project-groups.md`](../todo/2026-10-23-session-drawer-grew-past-500-by-project-groups.md)

Already answered by the tree: the drawer moved to `tddy-session-web` (#862), `SessionDrawer.tsx` is 333
lines, and the code issue it points to no longer exists. This node is the next to change the project
group, so it claims the entry and its wrap deletes it. The new header row is its own component so the
drawer does not grow back.

### ⚠ DURING — [`oversized-file-create-session-pane.md`](../../../packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md)

The composer reuses the form's field components; it does not render or extend `CreateSessionPane`
(581). Extracting the host picker out of `CreateSessionHostAndProjectFields` shrinks no budget but adds
none.

### ⚠ DURING — Session-web specs do not typecheck — [`2026-11-07-session-web-specs-do-not-typecheck.md`](../todo/2026-11-07-session-web-specs-do-not-typecheck.md)

New specs must typecheck on their own even though the package's lint excludes the folder.

### ⚠ DURING — doc drift in `session-drawer.md`

§ Pinned main entry names `tddy-session-web` / `src/routing/appRoutes.ts` for pieces that live in
`tddy-session-create-web` and `tddy-client-web`; the wrap corrects the implementation lines it touches.

## Affected Packages

- [tddy-session-web](../../../packages/tddy-session-web/docs/README.md) — project header row, "+", screen routing
- [tddy-session-create-web](../../../packages/tddy-session-create-web/README.md) — `OrchestratorComposer`, host field extraction
- [tddy-client-web](../../../packages/tddy-client-web/README.md) — route helpers
- [tddy-storybook](../../../packages/tddy-storybook/) — composer story

## Related Feature Documentation

- PRD: [PRD-2026-11-10-orchestrator-composer.md](../../ft/web/1-WIP/PRD-2026-11-10-orchestrator-composer.md)
- [Session drawer](../../ft/web/session-drawer.md)

## Summary

A "+" on each project header opens `#/sessions/project/<id>/new`: a centred composer (heading, prompt
box, then model · effort · provider account · host · attachments). Send starts an orchestrator for the
project with the text as its initial prompt and selects it.

## Background

The developer asked for a "+" that opens "an agentic screen with limited options", the prompt box
vertically centred (screenshot, 2026-11-10), with model + effort, provider account, host and attachments.

## Responsibility

- `SessionProjectGroupHeader` (toggle + "+" + a slot n11 fills) replacing the single header `<button>`.
- `withProjectHomeGroups` → every listed project gets a group.
- `OrchestratorComposer` + `useOrchestratorComposerStart` in `tddy-session-create-web`;
  `buildOrchestratorStartRequest(form)`.
- `CreateSessionHostField` extracted from `CreateSessionHostAndProjectFields`.
- Route `sessionsProjectNewPath(projectId)` / `parseSessionsProjectNew` in `tddy-client-web`; accepted by
  `isSessionsDrawerPath`; detail-area case in `SessionsDrawerScreen`.

## Boundaries

- Does **not** add the gear, the settings pane or stored defaults (n11 — the header leaves a slot).
- Does **not** change the New-session form or the pinned main entry.
- Does **not** add the role (n1) or effort (n3); it sends them.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `orchestrator-role` (#878) | `session_role: orchestrator` admitted for a person and placed on the home worktree | the composer's request sets it; the acceptance asserts the request and, against the in-memory backend, the started session | change admission, placement or the toggle n1 adds to New session |
| n3 `start-effort` (#880) | `StartSessionRequest.effort`, `CreateSessionEffortField`, levels in start options | the composer renders n3's effort field and sends `effort` | add or change the effort field or slot |

Both parents are green in this tree (2026-11-14), so the tests drive the **real** n1 role admission
surface (`ORCHESTRATOR_SESSION_ROLE`, `SessionEntry.session_role`) and the **real** n3 effort picker
(`useCreateSessionEffort` / `CreateSessionEffortField`, `AgentInfo.effort_levels`) — no doubles at
those seams. No test here reaches a `TODO(<other node>)`. n7 (`stack-plan-tools`) is only the line
neighbour: nothing here touches its `TODO(stack-plan-tools)` stubs.

**Inherited red, not this node's:** `tddy-session-create-web/src/projectHomeStartSpec.test.ts`
`buildProjectHomeStartSpec > carries no project, branch, stack or placement fields` fails on the base
— n3 added `effort` to `cliAgentChoices` and the test pins the key list without it. n3's to fix.

## Draft PR contract

Published in commit 2 (every unimplemented body marked `TODO(orchestrator-composer): implement`):

- **tddy-client-web** `src/appRoutes.ts`: `SESSIONS_PROJECT_ROUTE` (`/sessions/project`),
  `PROJECT_NEW_SEGMENT` (`new`), `sessionsProjectNewPath(projectId)` (implemented — a one-line
  builder), `parseSessionsProjectNew(pathname): string | null` (stub → `null`); re-exported from
  `tddy-web/src/routing/appRoutes.ts`. `isSessionsDrawerPath` accepting the path is green's.
- **tddy-session-create-web**
  - `OrchestratorComposer({ projectId, daemonInstanceId, sessionToken, onStarted })` (stub → `null`);
    clients from `useDaemonClient` as `ProjectHomeSetupPane`'s are.
  - `useOrchestratorComposerStart(client, onStarted): { start(request), starting, error }` (stub).
  - `orchestratorStartRequest.ts`: `OrchestratorComposerValues`,
    `buildOrchestratorComposerRequest(values): StartSessionRequestInit` (stub → `{}`),
    `orchestratorSessionTypeFor(agents, daemonInstanceId): SessionType | null` (stub → `null`).
  - `CreateSessionHostField({ daemons, daemonInstanceId, setDaemonInstanceId })` (stub → `null`; green
    moves the host select out of `CreateSessionHostAndProjectFields` and renders it from both).
  - All three modules exported from `index.ts`.
- **tddy-session-web**
  - `SessionProjectGroupHeader({ group, groupKey, collapsed, onToggle, onNew?, actions? })` — stub
    renders today's toggle button; `SessionProjectGroup` now renders it and passes `onNew` through
    (no behaviour change).
  - `SessionDrawer` prop `onNewOrchestrator?: (projectId) => void` (declared, not read yet).
  - `utils/projectHomeGroups.ts` `withEveryProjectGroup(groups, projects)` (stub → `groups`).
  - Screen case in `SessionsDrawerScreen` (route → `OrchestratorComposer`, "+" → navigate,
    `onStarted` → `handleSessionCreated`) is green's — nothing stubbed there.
- **Test ids** (both packages' `cypress/support/testIds.ts`): `sessionsProjectComposer(projectId)`,
  `TEST_IDS.sessionsProjectComposerInput` / `…Send` / `…Error`; session-web also
  `sessionsDrawerProjectNew(projectId)` and `TEST_IDS.createSessionEffortSelect`.
- Failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its request needs n1's role and n3's effort field
**Concurrent with:** n7, n9
**Blocks:** n11 `project-settings-pane` (adds the gear to this header and pre-fills this composer)

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] Header row + "+"; every project gets a group
- [ ] Route + screen routing
- [ ] Composer layout (vertically centred box; controls under it)
- [ ] Reused fields: model, effort, provider account, host (extracted), attachments
- [ ] Send → start → select; refusal keeps text
- [ ] ✅ Drawer todo ([entry](../todo/2026-10-23-session-drawer-grew-past-500-by-project-groups.md))
- [ ] Story

## Technical Changes

### State A (Current)

Re-verified 2026-11-14 against the tree after n1 and n3 went green (line numbers before this node's
contract commit):

- `tddy-session-web/src/SessionProjectGroup.tsx:20-46`: the whole header is one `<button>` (toggle).
- `withProjectHomeGroups` (`utils/projectHomeGroups.ts:81`, was :78 — n1 added `ORCHESTRATOR_ROLE` at
  :41) adds groups only for projects with a main; `SessionDrawer.tsx:116-120` applies it only when
  `onEnsureProjectHome` is given.
- Fields in `tddy-session-create-web`: `CreateSessionModelField.tsx`, `CreateSessionProviderField.tsx`,
  host select inside `CreateSessionHostAndProjectFields.tsx:36-56`, `CreateSessionAttachmentsSection.tsx`
  + `useSessionAttachments` (`hooks/useSessionAttachments.ts:220`); most not exported.
- **n1 added** `CreateSessionOrchestratorField.tsx` (`ORCHESTRATOR_SESSION_ROLE`) +
  `useCreateSessionOrchestrator.tsx`, and a **module-private** `buildOrchestratorStartRequest(form,
  features)` in `createSessionRequest.ts:235-246` (the New-session switch's request — no attachments).
- **n3 added** `CreateSessionEffortField.tsx` + `useCreateSessionEffort.tsx` (`effort` from the agent's
  served `effortLevels` / `defaultEffort`), `CreateSessionFormValues.effort` and `effort` in
  `cliAgentChoices`.
- `CreateSessionPane.tsx` is **591** lines (was 581).
- The daemon admits `session_role: orchestrator` from a person and places it on the home worktree
  (`tddy-agent-launch/src/orchestrator_start.rs`); `refuse_what_an_orchestrator_cannot_honour`
  refuses only sandbox and off-home placements — **attachments are allowed**.
- Routes `tddy-client-web/src/appRoutes.ts` (`SESSIONS_PROJECT_HOME_SETUP_ROUTE` pattern :41-92,
  `isSessionsDrawerPath` :104-110).
- Detail switch `SessionsDrawerScreen.tsx:390-439` (setup ? `ProjectHomeSetupPane` : unknown ? … :
  `SessionMainPane`); `handleSessionCreated` :293-312 is what a started session goes through.
- No centred composer exists (closest: `tddy-chat-web/src/LiveAgentChatView.tsx:214-231`, bottom bar).

### State B (Target)

- As Responsibility.

### Delta

- session-web: header row component, group rule, screen case, handler.
- session-create-web: composer, start hook, request builder, host field extraction, exports.
- client-web: route.
- storybook: `tddy-session-create-web/OrchestratorComposer.stories.tsx`.

## Implementation Milestones

- [ ] Route + header + group rule
- [ ] Composer layout + fields
- [ ] Start + select + refusal
- [ ] Story

## Testing Plan

**Level**: Cypress component (`mountWithRpc` + `anInMemoryRpcBackend`) for the drawer and composer;
bun unit for the request builder and group rule; route unit test in `appRoutes.test.ts`.

## Acceptance Tests

All written and failing for this node's missing implementation (2026-11-14).

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerProjectComposerAcceptance.cy.tsx`
- :79 `the plus on a project header opens the composer for that project`
- :92 `a project with no sessions and no main still shows its header and plus`
- :101 `sending starts an orchestrator with the text as its initial prompt and selects it`
- :124 `a refused start keeps the text and shows the daemon's reason`

### tddy-session-create-web — `packages/tddy-session-create-web/cypress/component/OrchestratorComposerAcceptance.cy.tsx`
- :30 `the prompt box is vertically centred in the pane`
- :39 `the composer offers exactly model, effort, provider account, host and attachments`

### tddy-session-create-web — `packages/tddy-session-create-web/src/orchestratorStartRequest.test.ts`
- :58 `the request names the orchestrator role, the project, the prompt, model, effort, account, host and attachments`

### tddy-web — `packages/tddy-web/src/routing/appRoutes.test.ts` (new cases)
- :394 `the project composer path parses and is a drawer path`

## Unit tests (TDD Red)

- `tddy-session-create-web/src/orchestratorStartRequest.test.ts` — no daemon-owned field (worktree,
  branch, placement, stack); untouched effort/account pass through as default/vendor; nothing attached
  sends `[]`; `orchestratorSessionTypeFor`: first `project_home_main` agent on the host, ignores other
  hosts, `null` when none (that last one passes on the stub).
- `tddy-session-create-web/cypress/component/OrchestratorComposer.cy.tsx` — an empty box cannot be
  sent; Enter starts the orchestrator with the text and calls `onStarted`; the host picker names the
  host (`daemonInstanceId`).
- `tddy-session-web/cypress/component/SessionProjectGroupHeader.cy.tsx` — "+" calls `onNew` without
  toggling; actions slot renders after the "+"; (guards that pass on the stub) the toggle is not the
  "+", no `onNew` → no "+".
- `tddy-session-web/src/utils/projectHomeGroups.test.ts` — `withEveryProjectGroup`: empty groups for
  every listed project in name order before Other; a project with no session gets one; no duplicate
  (passes on the stub).
- `tddy-web/src/routing/appRoutes.test.ts` — round-trips an encoded id; not a session id nor main's
  setup path; no project without `new` or without an id (the last two pass on the stub).
- Fixtures: `tddy-session-create-web/cypress/support/rpc/orchestratorComposerBackend.tsx`, page objects
  `…/pages/orchestratorComposerPage.ts` and `tddy-session-web/cypress/support/pages/projectComposerPage.ts`.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Composer in `tddy-session-create-web`**, where the reused fields and attachments live and are not
  exported.
- **Every listed project gets a group** so "+" is always reachable (previously a project with neither a
  session nor a main had no group).
- **Session type** = the first agent whose spec declares `project_home_main`, as main's setup form picks.
- **Builder named `buildOrchestratorComposerRequest`**, not `buildOrchestratorStartRequest` — n1 already
  has a module-private function of that name in `createSessionRequest.ts` with different semantics (no
  attachments); two exported/private twins would mislead. Orchestrator decision (developer: finish
  without stopping), 2026-11-14.
- **`withEveryProjectGroup` is a new helper, applied only when the drawer offers the "+"**
  (`onNewOrchestrator` given), instead of changing `withProjectHomeGroups`' rule. Keeps the existing
  "a project with no main and no session gets no group" behaviour for a drawer with no "+", and leaves
  #project-home's tests untouched. Orchestrator decision (developer: finish without stopping), 2026-11-14.
- **Composer takes clients from `useDaemonClient`** (props: `projectId`, `daemonInstanceId`,
  `sessionToken`, `onStarted`), the `ProjectHomeSetupPane` pattern, so the screen's detail case stays
  one line. Orchestrator decision (developer: finish without stopping), 2026-11-14.
- **The composer sends attachments**, unlike n1's New-session switch: the daemon's orchestrator
  refusals (`refuse_what_an_orchestrator_cannot_honour`) do not cover them. Orchestrator decision
  (developer: finish without stopping), 2026-11-14.
- **"Exactly five controls" is asserted as a set**: the composer's `<select>`s are exactly model,
  effort, provider, host (any order), the attachments section is present, and it has no checkbox — not
  as a list of every New-session control that must be absent. Orchestrator decision (developer: finish
  without stopping), 2026-11-14.
- **"Vertically centred" = the box's centre within 15% of the pane's height of the pane's centre**,
  with the composer filling its pane; the heading above and the controls under it shift it slightly.
  Orchestrator decision (developer: finish without stopping), 2026-11-14.

## Refactoring Needed

### From @red (TDD Red Phase)
### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### Change Validation (@validate-changes)
### Test Validation (@validate-tests)
### Production Readiness (@prod-ready)
### Code Quality (@analyze-clean-code)

## Successor PRs

n9 `feature/orchestrators/stack-web-by-id`.

## TODO

- [x] Record initial discovery (`2026-11-10-orchestrator-composer-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — skipped by developer instruction 2026-11-14
- [x] TDD Red — write failing unit/integration tests
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
