# 2026-11-07 — tddy-session-create-web's moved specs do not typecheck, so `Web lint` excludes it

**Category:** Lint gate gap
**Source:** `#web-split` 7/17 (PR #844), first `Web lint` run on the package

`tsc --noEmit` in `packages/tddy-session-create-web` reports 235 errors: 233 in `cypress/` — the
create-session specs and RPC support moved from `tddy-web`, which build responses as plain objects
(`{ sessions: [] }`, `{ id, label }`) where the generated message types (`ListSessionsResponse`,
`ModelInfo`, `ProjectEntry`, …) are expected — and one from `tddy-client-web/src/lib/debugMask.ts`
(TS7016, no declaration file for `debug`; fixed in #847 — 234 remain). `src/` itself is clean. The specs never type-checked in
`tddy-web` either (that package is excluded too); the per-package workflow this node had ran no
`typecheck`, so the gap surfaced only when `Web lint` took the package in.

**Why deferred:** fixing the fixture literals is a test-support rewrite, not this node's move.

**What closing it takes:** build the fixtures with `create(<Schema>, {...})` (or typed builders in the
package's `cypress/support/rpc/`), and delete the `tddy-session-create-web` entry from
`scripts/web-lint-exclusions.json`.
