# Changeset: Storybook package (`tddy-storybook`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — one new package depending on ten component packages; new stories per package; e2e consumers retargeted

**Stack**: `#web-split` 15/16 — branch `feature/web-split/storybook`, base `feature/web-split/session`

## Initial Discovery

[2026-11-07-storybook-package-initial-discovery.md](./2026-11-07-storybook-package-initial-discovery.md)

## Affected Packages

- **tddy-storybook** (new): `packages/tddy-storybook/` — sources, Cypress specs, page objects/drivers/fixtures, README, `docs/`, moved code-issue records.
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — the moved modules leave; consumers import from `tddy-storybook`; feature docs under `packages/tddy-web/docs/` for this area move to `packages/tddy-storybook/docs/` at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-storybook.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-storybook-package.md](../../ft/web/1-WIP/PRD-2026-11-07-storybook-package.md)

## Summary

Create the one Storybook for the web packages; it owns every story and depends on the component packages.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `packages/tddy-storybook`: Storybook 9 config (`.storybook/main.ts`, `preview.tsx` with the theme and port doubles), the moved stories (`App`, `Button`, `GhosttyTerminal`, `AgentChat`, and the other existing ones), and **at least one story per component package** for its main screen/component.
- A story-render smoke test (each story renders without error) in its PR check; `build-storybook` output consumed by `tddy-web`'s e2e scripts.
- The rule that no component package contains a `*.stories.tsx`.
- Its boundary rule test `packages/tddy-storybook/src/packageBoundary.test.ts` and `web-tddy-storybook.yml`.

## Boundaries

- No component behaviour or API change.
- Story ids the e2e terminal specs use stay the same.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, theme, shared helpers | renders with them | move UI primitives |
| `n5` clients (#842) | generated clients, `ClientsPort`/`NavigationPort`/`AuthPort` hooks, `mountWithClients` | all RPC/navigation/auth through them; specs mount with `mountWithClients` | add a transport or provider |
| `n7`–`n14` component packages | their exported components | stories render them against port doubles | change their APIs |
| `n2` react-commons (#839) | `testkit` (`stubPorts`, `AppContextStub`) | stories mount with it | — |

## Draft PR contract

First push (commit 2): the `tddy-storybook` skeleton (manifest exporting the surface in `## Responsibility`, Cypress/Vite config, workflow), the specs, page objects and drivers listed below `git mv`'d into the package with imports pointing at it, and the boundary rule — failing until the modules move.

## Green wave

**Wave:** 8 of 9
**Greenable independently:** yes — stories render published components against doubles
**Concurrent with:** none (only node in wave 8)
**Blocks:** `wiring` (e2e runs against this build)

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] `tddy-storybook` skeleton + workflow
- [ ] Modules moved; `tddy-web` (and sibling packages) import from `tddy-storybook`
- [ ] Specs, page objects, drivers moved and green from the package config
- [ ] Boundary rule
- [ ] Code-issue records and package docs moved

## Technical Changes

### State A (Current)

5 `*.stories.tsx` in `tddy-web` (`App`, `Button`, `GhosttyTerminal`, `AgentChat`, …); `.storybook/main.ts` in `tddy-web` (react-vite, tailwind, `@` alias, `ghostty-web` optimizeDeps); `cypress:e2e` builds Storybook and serves it on 9786.

### State B (Target)

`tddy-storybook` owns all stories; `tddy-web/.storybook` removed; e2e scripts build `tddy-storybook`.

### Delta

- `git mv` of the modules, specs and support; import rewrites; aliases; the upward reaches named in State A cut.

## Implementation Milestones

- [ ] `tddy-storybook` specs green
- [ ] `tddy-web` builds; consumers' scoped specs green

## Testing Plan

**Level:** Cypress component from the package's own config (existing assertions, mount helper swapped to `mountWithClients` + port doubles), `bun test` for moved pure suites, a boundary rule.

## Acceptance Tests

### tddy-storybook

- `src/storiesRender.test.ts` (or a Cypress e2e over the static build) — `every story renders without an error`.
- `src/storyPlacement.test.ts` — `no component package contains a story`.
- `tddy-web` `cypress/e2e/storybook.cy.ts`, `ghostty-terminal-stories.cy.ts` — pass against the moved stories (ids unchanged).
- `src/packageBoundary.test.ts` — `tddy-storybook imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Stories in one package** (developer's choice) — component packages stay free of Storybook deps.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-storybook-package-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`tddy-storybook` Cypress + `bun test` + boundary rule; `tddy-web` build + consumers' scoped specs) — verify 100% pass
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
