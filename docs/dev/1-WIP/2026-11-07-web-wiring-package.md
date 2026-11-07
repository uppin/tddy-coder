# Changeset: `tddy-web` as the wiring package

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 5 — several row-4 signals: app composition through the framework container (a design the stack has not exercised), leftover modules moved into several packages, the wiring rule, e2e retargeted

**Stack**: `#web-split` 16/16 — branch `feature/web-split/wiring`, base `feature/web-split/storybook`

**PR**: [#853](https://github.com/uppin/tddy-coder/pull/853)

## Initial Discovery

[2026-11-07-web-wiring-package-initial-discovery.md](./2026-11-07-web-wiring-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ⚠ DURING — URL-state routing follow-ups — [`2026-08-01-web-url-state-routing-follow-ups.md`](../todo/2026-08-01-web-url-state-routing-follow-ups.md)

Routing stays in `tddy-web`; recorded, not fixed here.

## Affected Packages

- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — becomes `app/` + `routing/` + shell + pages + e2e; leftovers leave; README and `docs/` rewritten at wrap to describe the wiring package.
- **every component package** (`tddy-ui-web`, `tddy-client-web`, `tddy-livekit-web`, `tddy-session-create-web`, `tddy-terminal-web`, `tddy-rpc-playground-web`, `tddy-chat-web`, `tddy-hosts-web`, `tddy-workspace-web`, `tddy-prstack-web`, `tddy-session-web`): receives the leftover modules its consumers need; may export a handler for the container.
- **tddy-desktop**: consumes the bundle; unchanged behaviour.
- `.github/workflows/web-tddy-web.yml` (from #838) — its test scope becomes build + app specs + e2e.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-web-wiring-package.md](../../ft/web/1-WIP/PRD-2026-11-07-web-wiring-package.md)

## Summary

`tddy-web` becomes composition only: the app container wires every package's handler and port implementations; leftovers move to their owners; a rule keeps it that way.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-web/src/app/`: the app container (`ReactAppContainer` from `tddy-react-commons`) composing each package's handler, the port implementations (HTTP/Tauri transports, `livekitPorts`, URL navigation, auth), shell, routes and `*AppPage`s.
- Leftover root `components/`, `hooks/`, `lib/`, `utils/`, `test-utils/` modules moved to the package that owns their consumers.
- `src/wiringBoundary.test.ts` — `tddy-web holds no feature component` (only `app/`, `routing/`, shell and pages).
- `tddy-web`'s PR check covers build, remaining specs (shell/app/routing) and e2e.


## Boundaries

- No package API change beyond exposing a handler where a package needs app-level state.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, theme, shared helpers | renders with them | move UI primitives |
| `n5` clients (#842) | generated clients, `ClientsPort`/`NavigationPort`/`AuthPort` hooks, `mountWithClients` | all RPC/navigation/auth through them; specs mount with `mountWithClients` | add a transport or provider |
| every node `n2`–`n15` | their packages, ports and stories | composes them | re-implement any of them |

## Draft PR contract

**As pushed (commit 2):** `src/app/composeApp.tsx` — `AppPorts` (clients, navigation, auth, presence, media),
`AppEnvironment`, `appHandler(environment)`, `composeApp(handler)` with `TODO(wiring)` bodies; failing
`src/wiringBoundary.test.ts` (feature modules still in `tddy-web/src`; `index.tsx` not composed through
`composeApp(appHandler(…))`) and `cypress/component/AppComposition.cy.tsx` (each route renders its package's screen
through the composed app, all ports doubled).

Originally planned:

First push (commit 2): `tddy-web/src/app/` composition signatures (`// TODO(wiring): implement`), the wiring rule test and `AppComposition.cy.tsx` — failing while feature components and leftovers remain in `tddy-web` and the app is not composed through the container.

## Green wave

**Wave:** 9 of 9
**Greenable independently:** yes — once every package exists on the base
**Concurrent with:** none (only node in wave 9)
**Blocks:** nothing (the sweep node follows)

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] App container composition + port implementations in `app/`
- [ ] Leftover modules moved to their owning packages
- [ ] Wiring rule test + `AppComposition.cy.tsx` green; app specs and e2e green
- [ ] Boundary rule
- [ ] `tddy-web` README/docs describe the wiring package

## Technical Changes

### State A (Current)

After `session`, `tddy-web` holds shell/routing/auth/app entry plus leftovers: root `components/` (AuthCallback, SessionWorkflowStatusCells, …), `hooks/`, `lib/`, `utils/`, `test-utils/`, ~9 app-level specs (`App`, `AppShell`, `SelectedHost*`, `ServerStreamFallback`, `ServingDaemon*`, `useConnectionStatus`, `useSessionAttachment`, `worktreeToolParity`).

### State B (Target)

`tddy-web` = `app/` + `routing/` + shell + pages + e2e.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [ ] Wiring rule green
- [ ] `tddy-web` builds; app specs and e2e green in its PR check

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-web

- `src/wiringBoundary.test.ts` — `tddy-web holds no feature component`.
- `cypress/component/AppComposition.cy.tsx` — `the app container composes every package's handler and the routes render each screen`.
- Remaining app specs (`App.cy.tsx`, `AppShell.cy.tsx`, `SelectedHostSelectionSurvivesReload`, …) and the e2e suite — pass.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Complexity 5 — design unsettled**: how much app state goes through the container versus ports is decided here; if it turns into a rewrite, stop and narrow to composition of the existing providers.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-web-wiring-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`tddy-web` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
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
