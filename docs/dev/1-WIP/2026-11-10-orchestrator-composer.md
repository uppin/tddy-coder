# Changeset: start an orchestrator from the project entry

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Feature
**Complexity**: 3 — web only, but three packages (session-web, session-create-web, client-web route) and a new screen reusing extracted form pieces
**Stack**: `#orchestrators` 8/11 · branch `feature/orchestrators/orchestrator-composer` · base `feature/orchestrators/stack-plan-tools` (#884)

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

## Draft PR contract

- Route helpers; `SessionProjectGroupHeader` with test ids `sessions-drawer-project-new-<id>`;
  `OrchestratorComposer` shell (`sessions-project-composer-<id>`, input
  `sessions-project-composer-input`, send `sessions-project-composer-send`); `buildOrchestratorStartRequest`
  signature (`TODO(orchestrator-composer)`).
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

- `SessionProjectGroup.tsx:20-46`: the whole header is one `<button>` (toggle).
- `withProjectHomeGroups` (`utils/projectHomeGroups.ts:78`) adds groups only for projects with a main.
- Fields in `tddy-session-create-web`: `CreateSessionModelField.tsx`, `CreateSessionProviderField.tsx`,
  host select inside `CreateSessionHostAndProjectFields.tsx:36-56`, `CreateSessionAttachmentsSection.tsx`
  + `useSessionAttachments` (`hooks/useSessionAttachments.ts:220`); most not exported.
- Routes `tddy-client-web/src/appRoutes.ts` (`SESSIONS_PROJECT_HOME_SETUP_ROUTE` pattern :41-93,
  `isSessionsDrawerPath` :104).
- Detail switch `SessionsDrawerScreen.tsx:390-398`.
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

### tddy-session-web — `packages/tddy-session-web/cypress/component/SessionsDrawerProjectComposerAcceptance.cy.tsx`
- `the plus on a project header opens the composer for that project`
- `a project with no sessions and no main still shows its header and plus`
- `sending starts an orchestrator with the text as its initial prompt and selects it`
- `a refused start keeps the text and shows the daemon's reason`

### tddy-session-create-web — `packages/tddy-session-create-web/cypress/component/OrchestratorComposerAcceptance.cy.tsx`
- `the prompt box is vertically centred in the pane`
- `the composer offers exactly model, effort, provider account, host and attachments`

### tddy-session-create-web — `packages/tddy-session-create-web/src/orchestratorStartRequest.test.ts`
- `the request names the orchestrator role, the project, the prompt, model, effort, account, host and attachments`

### tddy-web — `packages/tddy-web/src/routing/appRoutes.test.ts` (new cases)
- `the project composer path parses and is a drawer path`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Composer in `tddy-session-create-web`**, where the reused fields and attachments live and are not
  exported.
- **Every listed project gets a group** so "+" is always reachable (previously a project with neither a
  session nor a main had no group).
- **Session type** = the first agent whose spec declares `project_home_main`, as main's setup form picks.

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
