# Changeset: UI kit package (`tddy-ui-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages (new `tddy-ui-web`, import rewrites across `tddy-web`), new Cypress config + workflow, tailwind source scanning across packages

**Stack**: `#web-split` 3/16 — branch `feature/web-split/ui`, base `feature/web-split/react-commons` (#839)

## Initial Discovery

[2026-11-07-ui-kit-package-initial-discovery.md](./2026-11-07-ui-kit-package-initial-discovery.md)

## Prerequisites

### ⚠ DURING — Cypress runs without the React Compiler — [`2026-10-23-cypress-runs-without-the-react-compiler.md`](../todo/2026-10-23-cypress-runs-without-the-react-compiler.md)

The new package's `cypress.config.ts`/`vite.config.ts` copy `tddy-web`'s compiler switch exactly
(`CYPRESS_DISABLE_REACT_COMPILER=1` under Cypress). Recorded, not fixed here; this node must not make
the packages diverge on it.

## Affected Packages

- **tddy-ui-web** (new): `packages/tddy-ui-web/` — README, `docs/`, sources, Cypress specs, workflow.
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — loses `components/ui/*`, `lib/utils`,
  the theme CSS and the shared pure helpers/hooks; imports rewritten; `components.json` aliases.
- **root** `package.json` `workspaces`, `bun.lock`; `.github/workflows/web-tddy-ui-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-ui-kit-package.md](../../ft/web/1-WIP/PRD-2026-11-07-ui-kit-package.md)

## Summary

Move the presentational foundation into `tddy-ui-web` so component packages can render the app's
primitives without depending on `tddy-web`.

## Background

See the PRD. These modules import no generated code, no transport and no LiveKit.

## Responsibility

- `packages/tddy-ui-web` exports: the shadcn primitives (`button`, `inline-error`, `scroll-area`,
  `select`, `table`, `toast`, `tooltip`), `cn` (from `lib/utils`), the theme stylesheet
  (`tddy-ui-web/theme.css`, today's `src/index.css` tokens, fonts, tailwind imports and a `@source`
  for every package that renders tddy classes), `useIsMobile`, `useVisualViewport`, and the pure
  helpers from `lib/`/`utils/` that **two or more future packages** consume (the consumer count is
  measured in pass 2 and recorded; a helper only one feature uses moves later with that feature).
- Its Cypress config and support (`mount` with the theme applied), and the specs that cover what it
  owns: `Button.cy.tsx`, `Toast.cy.tsx`, `useVisualViewport.cy.tsx`, and the specs of moved helpers.
- The package boundary rule test for `tddy-ui-web` (no import of `tddy-web`, generated code,
  `@connectrpc/*`, `livekit-client`).
- `web-tddy-ui-web.yml`.

## Boundaries

- Does not move any feature component (`components/*` outside `ui/`), any `rpc/`, `gen/`, `routing/`.
- Does not change any component's API or look.
- Does not move stories (the storybook node owns them; `Button.stories.tsx` stays until then).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml`, the workspace ↔ workflow rule | adds `web-tddy-ui-web.yml` | change them |
| `n2` react-commons (#839) | — (branch predecessor only) | nothing | import it |

## Draft PR contract

First push (commit 2): the package skeleton (package.json exports, tsconfig, vite + Cypress config,
support file, workflow), the moved specs pointing at `tddy-ui-web` imports, and the boundary rule test
— failing because the modules have not moved yet (imports unresolved / module not found), while
`tddy-web` still builds from its own copies.

## Green wave

**Wave:** 2 of 9
**Greenable independently:** yes — the specs mount this package's own primitives; nothing from n2/n4.
**Concurrent with:** `feature/web-split/react-commons` (#839), `feature/web-split/livekit-rpc`
**Blocks:** `livekit`, `create-session`, `terminal`, `playground` and every later component package.

    n1 → n3      n3 → n6, n7, n8, n9 → …

## Scope

- [ ] Package skeleton + Cypress/Vite config (compiler switch identical to `tddy-web`)
- [ ] Primitives, `cn`, theme, shared hooks/helpers moved; `tddy-web` imports rewritten
- [ ] Tailwind sources cover every package that renders tddy classes
- [ ] Specs moved and green from the package config
- [ ] Boundary rule test
- [ ] `web-tddy-ui-web.yml`

## Technical Changes

### State A (Current)

`packages/tddy-web/src/components/ui/{button,inline-error,scroll-area,select,table,toast,tooltip}.tsx`
(512 lines), `src/lib/utils.ts` (`cn`), `src/index.css` (tailwind + `tw-animate-css` + shadcn css +
Geist font + theme tokens, light/dark), `hooks/useIsMobile.ts`, `hooks/useVisualViewport.ts`.
shadcn `components.json` aliases `@/components/ui`, `@/lib/utils`. `@/components/ui` has 11 importers,
`@/lib/utils` 7 (plus relative imports). 45 pure `lib/*.ts` modules import nothing app-internal.

### State B (Target)

`tddy-ui-web` owns those modules; `tddy-web` imports them from `tddy-ui-web` (resolved from source
via the same alias pattern as the other workspace packages, and `react` deduped to one copy).
`tddy-web/src/index.css` imports `tddy-ui-web/theme.css`.

### Delta

- New package; moved files with history (`git mv`); import rewrites in `tddy-web`; `components.json`
  `ui`/`utils` aliases; `vite.config.ts`/`tsconfig.json` aliases.

## Implementation Milestones

- [ ] `tddy-ui-web` specs green from its own config
- [ ] `tddy-web` builds; its specs that import primitives still pass (scoped: the moved-import specs)
- [ ] Boundary rule green

## Testing Plan

**Level:** Cypress component (the package's own config) + one `bun test` boundary rule.

## Acceptance Tests

### tddy-ui-web

- `packages/tddy-ui-web/cypress/component/Button.cy.tsx` — moved, unchanged assertions.
- `packages/tddy-ui-web/cypress/component/Toast.cy.tsx` — moved.
- `packages/tddy-ui-web/cypress/component/useVisualViewport.cy.tsx` — moved.
- `packages/tddy-ui-web/src/packageBoundary.test.ts` — `tddy-ui-web imports nothing from tddy-web,
  generated code, connect or livekit` (scans its `src/`).

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Helpers move by consumer count**, so the kit does not become a junk drawer of feature helpers.
- **Theme as a stylesheet export**, so each package's Cypress mount and the app share one theme.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-ui-kit-package-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`tddy-ui-web` Cypress + boundary rule; `tddy-web` scoped specs) — verify 100% pass
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
