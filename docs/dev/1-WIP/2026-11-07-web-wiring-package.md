# Changeset: `tddy-web` as the wiring package

**Date**: 2026-11-07
**Status**: 🚧 In Progress — green: wiring rule and `AppComposition` pass; validation pending
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

- [x] App container composition + port implementations in `app/`
- [x] Leftover modules moved to their owning packages
- [x] Wiring rule test + `AppComposition.cy.tsx` green; app specs and e2e green
- [x] Boundary rule
- [x] `tddy-web` README/docs describe the wiring package

## Technical Changes

### State A (Current)

After `session`, `tddy-web` holds shell/routing/auth/app entry plus leftovers: root `components/` (AuthCallback, SessionWorkflowStatusCells, …), `hooks/`, `lib/`, `utils/`, `test-utils/`, ~9 app-level specs (`App`, `AppShell`, `SelectedHost*`, `ServerStreamFallback`, `ServingDaemon*`, `useConnectionStatus`, `useSessionAttachment`, `worktreeToolParity`).

### State B (Target)

`tddy-web` = `app/` + `routing/` + shell + pages + e2e.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [x] Wiring rule green
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

## Green notes (deviations)

- **Design narrowed to composition of the existing providers.** `appHandler` builds the React-free ports (`navigation`, `presence`, `media`, `roomRpc`, `nativeFileDrop`). The `clients` and `auth` ports are optional on `AppPorts`: in production the stateful providers (`RpcTransportProvider`, `ConnectionProviders`, `AuthProvider`, `SelectedDaemonProvider`) still build them, because transport, connection wires, the selected-daemon directory and `useAuth` are hooks over React state; building them in the handler is a rewrite. A handler that supplies them (the specs' doubles) replaces those providers. `TODO(wiring)` in `app/ports.ts`.
- `App` moved to `src/app/App.tsx` and reads the location from the navigation port (`tddy-client-web`'s `useAppLocation`), so the composed app routes through the handler's port; the hash-backed port is the production default.
- `rpc/` (transports, registry, selected-daemon directory, `appClientsPorts`) and `hooks/` (auth) are port implementations: moved under `src/app/rpc/` and `src/app/auth/`, so the wiring rule needed no allowlist entry. `AuthCallback`, `HostStartPassphrasePrompt` (the create-session/hosts slot) moved to `src/app/`; `TasksDrawerScreen` and `StandaloneConnectionScreen` became `TasksAppPage` / `StandaloneConnectionAppPage` (pages composing several packages).
- Leftovers moved out: `SessionWorkflowStatusCells` (no reader) to `tddy-session-web`; `fileUploadChunks.test.ts` to `tddy-livekit-web`; `prStackOrchestrators.test.ts` to `tddy-prstack-web`.
- Red test adjusted: `AppComposition.cy.tsx` intercepts `/api/config` (a daemon-mode page) and asserts `rpc-playground-home` — `rpc-playground-screen` is not a testid the screen has.
- `tddy-web/package.json` gains the `tddy-react-commons` workspace dependency; `test:unit` is `bun test src`.

## Inherited failures (fail identically on base `6beb388bf`, `feature/web-split/storybook`)

- `cypress/component/CodeHoverCard.cy.tsx` > "a long hover stays within its bounds and keeps its actions visible": `card height: expected 2487 to be at most 320.5`.
- `cypress/component/SessionsSshExecAcceptance.cy.tsx` > "lists Host A's aliases and an empty local choice": `expected [ '' ] to deeply equal [ '', 'buildbox' ]`; and the SSH host option test: `SSH host option "buildbox": expected [ '' ] to include 'buildbox'`.

## Decisions (green)

- `clients`/`auth` optional on `AppPorts` is a source selection, not a default: absent means the in-tree providers build them (always, in the real app); supplying only one throws. A consumer reading a missing port still throws naming its provider (`useClientsPort`, `useAuth`).
- **developer, 2026-11-08: fallbacks removed.** `AppClientsPorts` outside a composed app throws naming the navigation and LiveKit ports and `AppPortsScope` (`app/portsInScope.ts`, `portsInScope.test.ts`); `roomRpc` is required on `AppPorts` and an absent one throws naming it. No hash-navigation or module-LiveKit default remains in `src/`. The Cypress `mount` (support/component.ts) supplies the ports explicitly: the page's hash navigation and the real LiveKit ports, not doubles, because the specs relying on them exercise LiveKit's transport-factory override; `AppComposition.cy.tsx` supplies `aRoomRpcDouble`.

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
- [x] TDD Green — implement with quality code
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
