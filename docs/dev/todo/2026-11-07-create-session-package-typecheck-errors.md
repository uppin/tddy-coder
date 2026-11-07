# 2026-11-07 — `tddy-session-create-web` does not typecheck (235 errors)

**Category:** Type errors (test support)
**Source:** `#web-split` 7/17 (PR #844), `/green`
**Detected:** `cd packages/tddy-session-create-web && ../../dev bunx tsc --noEmit -p .` — 235 `error TS`
lines at `feature/web-split/create-session`, 2026-11-07: 234 under `cypress/` (protobuf message literals
without `$typeName`, untyped page-object chains — carried over from the `tddy-web` specs they were moved
from), 1 in `tddy-client-web/src/lib/debugMask.ts` (no declaration for `debug`). `src/` itself is clean.

**Why deferred:** `tsc` is not a gate in this repo (`tddy-web` carries over a thousand of the same kind),
the specs pass, and fixing them is rewriting the moved specs' fixtures — not this node's move.

**What closing it takes:** build fixtures with `create(<Schema>, …)` instead of literals, type the page
objects, and add `@types/debug` (or a local declaration, as `tddy-livekit-web` did in #843) — then make
`bun run typecheck` part of the package's PR check so it stays at zero.
