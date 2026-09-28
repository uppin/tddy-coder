# oversized-file: useModelRegistryFanOut.ts

**Location:** `packages/tddy-web/src/components/models/useModelRegistryFanOut.ts`
**Category:** oversized-file
**Detected:** 2026-09-28 by `/pr-wrap` step 3.5 file-length gate on PR #555 (`#subagent-control` 3/5)
**Metrics:** **615 production lines** · budget 500 · **1.2× over**
**Thresholds breached:** length 615 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-09-28 | 598 | first detection — already over before PR #555; this package had no `docs/code-issues/` yet |
| 2026-09-28 | 615 | +17 in PR #555 (`usageNotes` on the create/update input types and RPC payloads) — growth consented to be deferred, see `docs/dev/todo/2026-09-28-web-model-fan-out-over-budget.md` |

## What would close it

Hand TS split (the `code-restructuring` engine is Rust-only): the hook's three concerns — RPC
client fan-out per host, the projected row models, and the create/update command payloads — are
the cohesive groups. Green baseline before, mechanical moves only, same green after.
