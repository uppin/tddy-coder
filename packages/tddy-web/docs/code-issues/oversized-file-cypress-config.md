# oversized-file: cypress.config.ts

**Location:** `packages/tddy-web/cypress.config.ts`
**Category:** oversized-file
**Detected:** 2026-11-07 by `/pr-wrap` step 3.5 file-length gate on PR #842 (`#web-split` 5/16)
**Metrics:** **1014 lines** (non-test source, whole file) · budget 500 · **2.03× over**
**Thresholds breached:** length 1014 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-07 | 1014 → 1014 | PR #842 re-points its generated-code imports to `tddy-client-web` without growing it; alert-only |

## What would close it

Move the Node-side `setupNodeEvents` tasks (fixture daemons, servers, file helpers) out of the config
into modules under `cypress/plugins/`, leaving the config as the `defineConfig` call that registers
them. By hand, green baseline first (a handful of e2e and component specs that use the tasks).

## Verified by hand

2026-11-07 — `wc -l` at the wrap of PR #842 (1014) and at its base `feature/web-split/livekit-rpc` (1014).
