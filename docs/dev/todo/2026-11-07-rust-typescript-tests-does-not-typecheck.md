# 2026-11-07 — tddy-rust-typescript-tests does not typecheck, so `Web lint` excludes it

**Category:** Lint gate gap
**Source:** `#web-split` 6/17 (PR #843), web CI redesign

`tsc --noEmit` in `packages/tddy-rust-typescript-tests` fails with `TS2688: Cannot find type definition
file for 'bun-types'`: its tsconfig names a types package the workspace does not install.

**Why deferred:** fixing it means adding a dependency (`bun-types` / `@types/bun`), which needs consent.

**What closing it takes:** install the bun type definitions (or point `types` at what is installed), fix
any errors that surfaces, add a `typecheck` script and delete the entry in `scripts/web-lint-exclusions.json`.
