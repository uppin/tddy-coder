# 2026-11-07 — tddy-session-web's drawer-driven Cypress specs do not typecheck

**Category:** Lint gate gap (test support)
**Source:** `#web-split` 18/20 (PR #862), wrap
**Detected:** `cd packages/tddy-session-web && ../../dev bun run typecheck` — **182** `error TS` lines at
`feature/web-split/session-drawer`, 2026-11-07: 181 under `cypress/` and one in `tddy-tauri-web` (fixed in
#852). `src/` is clean.

The `cypress/` errors are in the 111 drawer-driven specs and their support moved from `tddy-web` (which
never type-checked): RPC handlers and fixtures returning plain objects where generated response
messages are expected (TS2322/TS2345, mostly the `PrStack*`, `Session*TargetRows*`,
`SessionInspector*`, `CreateSession*` specs and `support/rpc/{responses,daemonSessionHostBackend,
vncBackend,screenSharingBackend}.ts`), a handful of stale props or arity (TS2554/TS2339/TS2551), and
optional fields read without a guard (TS18048).

**Why deferred:** a fixture rewrite across ~60 moved spec files is test-support work, not this node's
move; the earlier package nodes deferred the same debt (`2026-11-07-session-create-web-specs-do-not-typecheck.md`,
`2026-11-07-workspace-web-specs-do-not-typecheck.md`).

**What closing it takes:** build responses with `create(<method>.output, {...})` in the specs and the
support backends, fix the stale props, then delete the
`tddy-session-web` entry from `scripts/web-lint-exclusions.json`.
