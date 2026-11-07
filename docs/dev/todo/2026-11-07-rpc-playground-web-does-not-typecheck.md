# 2026-11-07 — tddy-rpc-playground-web typechecks only through tddy-client-web's `debug` import, so `Web lint` excludes it

**Category:** Lint gate gap
**Source:** `#web-split` 9/17 (PR #846), wrap on the redesigned `web.yml`

`tsc --noEmit` in `packages/tddy-rpc-playground-web` reports **1** error, and it is not in this
package: `../tddy-client-web/src/lib/debugMask.ts` (TS7016) — no declaration file for `debug`. The
package's own `src/` and `cypress/` are clean (the one local error, an untyped `unary(m)` in
`src/invoke.test.ts`, was fixed at wrap).

**Why deferred:** the `debug` typing belongs to `tddy-client-web`, shared with
`2026-11-07-session-create-web-specs-do-not-typecheck.md` and
`2026-11-07-terminal-web-does-not-typecheck.md`; `@types/debug` is a new dependency and needs consent.

**What closing it takes:** give `tddy-client-web` the `debug` types (`@types/debug`, or a local
declaration as `tddy-livekit-web` has), then delete the `tddy-rpc-playground-web` entry from
`scripts/web-lint-exclusions.json`.
