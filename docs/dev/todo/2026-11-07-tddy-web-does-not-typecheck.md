# 2026-11-07 — tddy-web does not typecheck, so `Web lint` excludes it

**Category:** Lint gate gap
**Source:** `#web-split` 6/17 (PR #843), web CI redesign

`tsc --noEmit` in `packages/tddy-web` reports ~1300 pre-existing errors, so the package has no
`typecheck` script and sits on `scripts/web-lint-exclusions.json`. `Web lint` therefore type-checks
every web package except the app itself.

**Why deferred:** the errors predate the split and span most of the app; the `#web-split` nodes move
code out of `tddy-web`, and each new package must typecheck clean, so the count shrinks as the stack lands.

**What closing it takes:** fix the remaining errors, add `"typecheck": "tsc --noEmit"` to
`packages/tddy-web/package.json`, and delete its exclusion entry (`scripts/web-ci.ts lint` fails while
both exist).
