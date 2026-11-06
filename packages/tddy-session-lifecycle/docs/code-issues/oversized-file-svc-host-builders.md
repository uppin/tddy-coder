# oversized-file: svc_host_builders.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs`
**Category:** oversized-file
**Detected:** 2026-11-02 — `/pr-wrap` step 3.5 file-length gate (PR #790)
**Metrics:** **503 production lines** (2026-11-02) · budget 500 · counted to the first `#[cfg(test)]`
**Thresholds breached:** length 503 > 500
**Status:** Open — **unclaimed**; split deferred until the `#agent-tools` stack lands
(`docs/dev/todo/2026-11-02-svc-host-builders-over-length-budget.md`)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-02 | 503 | 500 → 503 in PR #790 (`#agent-tools` 1/12): the workspace provisioner is built per `<tddyhome>` with `for_tddy_home`. PR #791 also edits this file, so the split waits for the stack |

## What would close it

The file under 500 production lines. Plan the seam with `restructure anchors` after the stack has
merged; no seam is designed yet.
