# 2026-11-07 — tddy-workspace-web's moved Cypress support does not typecheck, so `Web lint` excludes it

**Category:** Lint gate gap (test support)
**Source:** `#web-split` 13/17 (PR #849), wrap on the redesigned `web.yml`
**Detected:** `cd packages/tddy-workspace-web && ../../dev bun run typecheck` — **43** `error TS` lines
at `feature/web-split/workspace`, 2026-11-07, every one under `cypress/`:

- `cypress/support/rpc/modelRegistryBackend.ts` (12) and `cypress/support/drivers/daemonSettingsDriver.tsx` (8):
  handlers return plain objects where generated response messages are expected (TS2322/TS2345).
- `ProjectLocalGh.cy.tsx` (7), `ProjectAccountsAcceptance.cy.tsx` (6), `ProjectsScreenAcceptance.cy.tsx` (5),
  `projectsAppPageFixtures.tsx` (3), `ProjectsVisibleErrorsAcceptance.cy.tsx` (2): the same plain-object
  responses, plus a Sinon `Agent` passed where a `Promise`-returning callback is typed.

`src/` is clean (the one `src/` error, an incomplete `settingsForm.test.ts` fixture, was fixed at wrap).
The specs never type-checked in `tddy-web` either (that package is excluded).

**Why deferred:** rewriting moved fixtures is test-support work outside this node's move; it is the
same kind of debt as `2026-11-07-session-create-web-specs-do-not-typecheck.md` and
`2026-11-07-terminal-web-does-not-typecheck.md`.

**What closing it takes:** build responses with `create(<Schema>, {...})` in the backends, driver and
fixtures; type the Sinon stub as the callback's signature; then delete the `tddy-workspace-web` entry from
`scripts/web-lint-exclusions.json`.
