# Changeset: PR-stack package (`tddy-prstack-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress — green pushed; validation pending
**Type**: Architecture Change
**Complexity**: 4 — two packages (new package; sessions consumer), ~40 specs moved, three upward reaches cut, an inherited oversized-file record

**Stack**: `#web-split` 13/16 — branch `feature/web-split/prstack`, base `feature/web-split/workspace`

**PR**: [#850](https://github.com/uppin/tddy-coder/pull/850)

## Initial Discovery

[2026-11-07-prstack-package-initial-discovery.md](./2026-11-07-prstack-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ⚠ DURING — oversized `PrStackScreen` — `packages/tddy-web/docs/code-issues/oversized-file-pr-stack-screen.md`

Moves with the file; recorded, not fixed here.

## Affected Packages

- **tddy-prstack-web** (new): `packages/tddy-prstack-web/` — sources, Cypress specs, page objects/drivers/fixtures, README, `docs/`, moved code-issue records.
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the moved modules leave; consumers import from `tddy-prstack-web`; feature docs under `packages/tddy-web/docs/` for this area move to `packages/tddy-prstack-web/docs/` at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-prstack-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-prstack-package.md](../../ft/web/1-WIP/PRD-2026-11-07-prstack-package.md)

## Summary

Move the PR-stack screen, planned-PR panels, PR-stack chat/inspector and workflow chat into their own package.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-prstack-web` exports `PrStackScreen`, the planned-PR list/panel/rows, `PrStackChat`, `PrStackInspector`, the start-session modal, `WorkflowChat` views and their hooks.
- Moved specs (~40: `PrStack*`, `PlannedPr*`, `WorkflowChat*`) and fixtures.
- Its boundary rule test `packages/tddy-prstack-web/src/packageBoundary.test.ts` and `web-tddy-prstack-web.yml`.

## Boundaries

- Does not move create-session (#844) or chat (`feature/web-split/chat`) — it imports them.
- Does not move the session-side PR-stack entry points (drawer rows) — `feature/web-split/session`.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, theme, shared helpers | renders with them | move UI primitives |
| `n5` clients (#842) | generated clients, `ClientsPort`/`NavigationPort`/`AuthPort` hooks, `mountWithClients` | all RPC/navigation/auth through them; specs mount with `mountWithClients` | add a transport or provider |
| `n6` livekit (#843) | the presenter room via `MediaPort` (`usePresenterRoom`) | `PrStackChat` consumes it | import `livekit-client` |
| `n7` create-session (#844) | `CreateSessionDialog`, `CreateSessionPane`, `baseBranchChoice` | start-session modal opens them | move create-session |
| `n10` chat | `tddy-chat-web` | PR-stack chat renders it | move chat |
| `n11`, `n12` | — (branch predecessors only) | nothing | — |

## Draft PR contract

**As pushed (commit 2):** skeleton, workflow, boundary rule; **1** specs whose session imports are prstack,
create-session or the presenter room, plus the `prstack/*.test.ts` suites, `git mv`'d — failing until the
modules move. Specs mounting the sessions drawer stay for `feature/web-split/session`.

**Correction (commit 3):** the drawer filter left only `PrStackStartSessionDocAttachmentsAcceptance` here — **36** of the
40 PR-stack specs drive PR-stack through `SessionsDrawerScreen`, so they are integration specs of the drawer
and travel with `feature/web-split/session`. `WorkflowChatPresenterRoomAcceptance` and `WorkflowChatResumeAcceptance` mount `WorkflowChatScreen`
(moves with this package) and are moved now; `WorkflowChatViewRoutingAcceptance` routes through the drawer and stays. So the package has a component suite of its own, green **adds
direct-mount specs** (fluent style, `cy.mountWithClients` + port doubles) for `PrStackScreen` (planned-PR list
renders and reorders), `PrStackChat` (streams a reply, shows an elicitation) and `PlannedPrList` (start-session
opens `CreateSessionDialog`) — each asserting behaviour the drawer specs already cover, from the package's own
entry points.

Originally planned:

First push (commit 2): the `tddy-prstack-web` skeleton (manifest exporting the surface in `## Responsibility`, Cypress/Vite config, workflow), the specs, page objects and drivers listed below `git mv`'d into the package with imports pointing at it, and the boundary rule — failing until the modules move.

## Green wave

**Wave:** 6 of 9
**Greenable independently:** yes — specs mount PR-stack against in-memory RPC and media doubles; create-session and chat are published packages
**Concurrent with:** `workspace`
**Blocks:** `session`

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [x] `tddy-prstack-web` skeleton + workflow
- [x] Modules moved; `tddy-web` (and sibling packages) import from `tddy-prstack-web`
- [x] Specs, page objects, drivers moved and green from the package config
- [x] Boundary rule
- [x] Code-issue records moved (package feature docs move at wrap)

## Technical Changes

### State A (Current)

`components/sessions/prstack/` (3.4k non-test, 6k with tests) imports `../CreateSessionDialog`, `../CreateSessionPane`, `../usePresenterLiveKitRoom`, chat once, `attachments/` twice; `PrStackScreen` recorded oversized.

### State B (Target)

`tddy-prstack-web` owns PR-stack; sessions import it.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [x] `tddy-prstack-web` specs green
- [x] `tddy-web` builds; consumers' scoped specs green

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-prstack-web

- Moved: `PrStack*.cy.tsx`, `PlannedPr*.cy.tsx`, `WorkflowChat*.cy.tsx` — unchanged assertions.
- `src/packageBoundary.test.ts` — `tddy-prstack-web imports nothing from tddy-web or livekit-client`.

## Green notes (2026-11-07)

Verified (scoped): `tddy-prstack-web` `bun test src` 149 pass (incl. boundary); its 6 Cypress specs one at a time
(3 moved + 3 direct-mount, 10 tests, all pass); `scripts/livekit-import-boundary` + `web-workflows` 4 pass; `tddy-web`
`bun run build` ok, `test:unit` 653 pass; 8 drawer-driven specs in `tddy-web` (Reorder, ViewRouting, StartSessionModal,
ChatStreaming, Chat, PlannedPrList, WorkflowChatViewRouting, StartSessionDocAttachments) pass. Typecheck: no errors in
`src/*.tsx` or `cypress/`; the remaining errors are the moved tests' and the builders' proto-message typing, carried
verbatim from `tddy-web` (pre-existing pattern), plus two in dependency packages.

Deviations from the draft contract:

- **Surface names.** The boundary test listed `PrStackInspector` and `WorkflowChat`; neither exists as a module (the
  inspector is `SessionInspectorDrawer`'s, in the session package). The list is now the real exports: `PrStackScreen`,
  `PrStackChat`, `PlannedPrList`, `PlannedPrPanel`, `WorkflowChatScreen`.
- **`PrStackStartSessionDocAttachmentsAcceptance` stays in `tddy-web`**: it drives the drawer, like the other 35. No
  PR-stack spec that mounts the drawer moved; the package's own suite is the 3 moved specs plus 3 new direct-mount
  specs (`PrStackScreenDirectMount`, `PrStackChatDirectMount`, `PlannedPrListStartSessionDirectMount`).
- **`WorkflowChatPresenterRoomAcceptance` rewritten** to mount `WorkflowChatScreen` with a media-port double that
  records the hint it is asked to open a room for — it used `SessionMainPane`, which stays in `tddy-web`. The
  assertion is the same contract (the screen opens its own presenter room from the attachment).
- **Upward reaches cut beyond the three planned:** `HostStartPassphraseScope` (the Hosts screen's) is no longer
  mounted by `PrStackScreen` — the host application provides `StartPassphraseProvider`, as it does for
  `CreateSessionPane` (`workflowViews` wraps the screen; the package's Cypress support provides a no-op prompt);
  `SessionMetadata` became the package's own `StackChildMetadata` (the three fields the join reads, which
  `tddy-web`'s parsed shape satisfies); `resolveNodeSession` (+ test) moved in — its only consumer was the list.
- **Test support copied, not moved**, as earlier packages did: `prStackScreenPage`, `agentChatPage`, `acpSession`,
  the `aPlannedNode`/`aStackPlanJson` fixtures and a trimmed `testIds` (the 36 drawer specs still read the originals
  in `tddy-web`). `aStackNode`, `aStackChildSession`, `aBranchResolution` moved to `test-utils/builders.ts` (no
  `tddy-web` reader); `aSessionEntry` is duplicated there.
- **`PrStackScreen.tsx` 705 -> 704 lines** (not grown); the record moved with it.
- Feature docs under `docs/ft/` and `packages/tddy-web/docs/` still name the old `components/sessions/prstack/` paths
  — a wrap task.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Workflow chat moves here** — it is PR-stack's chat routing.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-prstack-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`tddy-prstack-web` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
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
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
