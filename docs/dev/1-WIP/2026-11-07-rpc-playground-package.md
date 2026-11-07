# Changeset: RPC playground package (`tddy-rpc-playground-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 2 — one new package and a handful of moved symbols; unit + two Cypress specs; `tddy-web` keeps the page

**Stack**: `#web-split` 9/16 — branch `feature/web-split/playground`, base `feature/web-split/terminal`

**PR**: [#846](https://github.com/uppin/tddy-coder/pull/846)

## Initial Discovery

[2026-11-07-rpc-playground-package-initial-discovery.md](./2026-11-07-rpc-playground-package-initial-discovery.md)

## Affected Packages

- **tddy-rpc-playground-web** (new): `packages/tddy-rpc-playground-web/` — `RpcPlaygroundScreen`, `invoke`, `registry`, `protoMessageJson`, `test-fixtures/`, their `bun test` suites, specs, README, `docs/`.
- **tddy-web**: keeps `RpcPlaygroundAppPage` (shell, sign-in, presence — wiring), importing the screen from the package.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-rpc-playground-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-rpc-playground-package.md](../../ft/web/1-WIP/PRD-2026-11-07-rpc-playground-package.md)

## Summary

Move the RPC playground screen and its helpers into its own package; the app page that hosts it stays as wiring.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-rpc-playground-web` exports `RpcPlaygroundScreen` and `buildServiceRegistry` / `invokeRpc` / `protoMessageJson` helpers.
- Moved specs `RpcPlaygroundScreen.cy.tsx`, `RpcPlaygroundUrlStateAcceptance.cy.tsx`, moved `bun test` suites, boundary rule, workflow.

## Boundaries

- `RpcPlaygroundAppPage` (AppShell, GitHub login, room participants, host presence) stays in `tddy-web`: it composes the shell and LiveKit-backed presence — wiring.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives | renders with them | move them |
| `n5` clients (#842) | generated reflection code, `useHostConnection().transport()`, `NavigationPort` (URL state) | issues calls over the port's raw transport; URL state through navigation | add a transport |
| `n6`–`n8` | — (branch predecessors only) | nothing | — |

## Draft PR contract

**As pushed (commit 2):** skeleton, workflow, boundary rule; the two specs and the three `bun test` suites
(`invoke`, `protoMessageJson`, `registry`) plus `test-fixtures/` `git mv`'d into the package — failing
until the modules move. The suites still import `tddy-web`'s `src/test-utils` fake transports; green moves
those (or their playground-only part) with the package. Exported names are `buildRegistry`/`findMethod`
(not `buildServiceRegistry` as `## Responsibility` first said).

Originally planned:

First push (commit 2): package skeleton + workflow, the two specs and three `bun test` suites `git mv`'d with imports pointing at the package, boundary rule — failing until the modules move.

## Green wave

**Wave:** 4 of 9
**Greenable independently:** yes — the specs mount the screen with `mountWithClients` and a reflection backend
**Concurrent with:** `livekit` (#843), `create-session`, `terminal`
**Blocks:** nothing (`wiring` and `storybook` consume it)

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] Package skeleton + workflow
- [ ] Screen + helpers moved; the page imports the package
- [ ] Specs and suites green from the package
- [ ] Boundary rule

## Technical Changes

### State A (Current)

`tddy-web/src/rpc-playground/` (849 lines): `RpcPlaygroundScreen.tsx`, `RpcPlaygroundAppPage.tsx`, `invoke.ts`, `registry.ts`, `protoMessageJson.ts`, `test-fixtures/`, three `bun test` suites. The page imports `GitHubLoginButton`, `AppShell`, `authProvider`, `useCapabilityAvailability`, `useRoomParticipants`, `presenceIdentity`, `routing/*`, `rpc/{connections/registry,hostDirectory/useHostPresence,selectedDaemon,transportProvider}`.

### State B (Target)

Screen and helpers in the package; the page in `tddy-web` renders it inside the shell.

### Delta

- `git mv` of four modules, fixtures, suites and two specs.

## Implementation Milestones

- [ ] Package suites + specs green

## Testing Plan

**Level:** `bun test` (helpers) + Cypress component (screen) from the package; boundary rule.

## Acceptance Tests

### tddy-rpc-playground-web

- `cypress/component/RpcPlaygroundScreen.cy.tsx`, `RpcPlaygroundUrlStateAcceptance.cy.tsx` — moved, unchanged assertions.
- `src/{invoke,protoMessageJson,registry}.test.ts` — moved.
- `src/packageBoundary.test.ts` — `tddy-rpc-playground-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Page stays as wiring** — it composes shell, sign-in and presence; the reusable part is the screen.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-rpc-playground-package-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`bun test` + Cypress + boundary rule in `packages/tddy-rpc-playground-web`) — verify 100% pass
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
