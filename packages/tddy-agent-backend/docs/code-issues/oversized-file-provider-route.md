# oversized-file: provider_route.rs

**Location:** `packages/tddy-agent-backend/src/provider_route.rs`
**Category:** oversized-file
**Detected:** 2026-11-04 by `/pr-wrap` step 3.5 file-length gate on PR #801 (`#agent-tools` 12/19)
**Metrics:** **502 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.00× over**
**Thresholds breached:** length 502 > 500
**Status:** Closed by #819 (186 production lines) — delete at wrap

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-05 | 502 → 186 | **closed by n13** (#819): `Harness`, `IDS`, `ALL`, the matrix and the three route bodies left for `provider_route/by_spec.rs` (320 lines) and `provider_route/support.rs`; delete this record at wrap |
| 2026-11-04 | 492 → 502 | first detection — PR #801 crossed it: `Harness::id` reads a new `Harness::IDS` table (11 rows) instead of its `match`, so the source scan of agent-name gates sees a value table. Not split: the developer deferred all code restructuring of the `#agent-tools` stack to its sweep node (2026-11-04) |

## What would close it

Node n13 `route-support` ([#819](https://github.com/uppin/tddy-coder/pull/819)) (`#agent-tools`) removes `provider_route::Harness` and selects route bodies by the spec's `RouteSupport`; the enum, `IDS`, `ALL` and the `route_support` matrix leave the file with it. Re-measure when n13 lands.
