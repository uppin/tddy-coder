# oversized-file: index.rs — the workspace index, its roots, gates and warm state in one module

**Location:** `packages/tddy-index-daemon/src/index.rs`
**Category:** oversized-file
**Detected:** 2026-10-06 by the `/pr-wrap` file-length gate on #591 (`#sharpen` 4/8)
**Metrics:** **508 production lines** (491 at the #591 merge base, `e2e69077`; the +17 is the
`wait_heartbeat` field and its two accessors) · budget 500
**Restructure:** required
**Status:** Open — over budget, not yet split

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-06 | 508 | first detection, on #591. 491 → 508 (+17): `WorkspaceIndex::wait_heartbeat`, `with_wait_heartbeat` and the `wait_heartbeat` field, carried so a run queued on a root can beat at the host's cadence. The logic of the heartbeat lives in `operations.rs` (`hold_saying_so`) and `tddy-code-restructuring`; only the field and its accessors are here. Split deferred (see below) |

## What the gate found

`index.rs` was 491 production lines at the #591 merge base — nine under the budget — and #591's
seventeen lines of wiring took it to 508. The file holds `WorkspaceIndex`: the warm `LspRegistry`, the
per-root `RootState` (gate, warmth, graph latch, plan store), the complexity cache, the plan stores and
the flusher — several concerns that already have their own types (`RootState`, `GraphLoad`,
`PlanStore`, `InMemoryComplexityCache`) but share one module with the methods that reach them.

## Why it was not split in #591

The node is a feature node (a heartbeat), not a decomposition, and the crossing is +17 lines of
wiring. The developer chose to **defer with a record** (2026-10-06) rather than expand the PR's scope
and risk with an engine-driven split mid-stack. No other open `#sharpen` node's own commits touch this
file, so the stack-overlap stop does not apply — this is a scope decision, not a conflict one.

## What would close it

Split along the seams the module already has: the per-root state and its gate/queue (`RootState`,
`hold`, `client_for`, `record_use`, `warm_workspaces`, `warmth_of`, `graph_load_of`) is one candidate;
the plan-store plumbing (`plans_of`, `flush_plans`, `reresolve_loaded_plans`) another; the complexity
cache access (`complexity_scores`) a third. A probe per seam first (`restructure check --deep`), then
engine moves. When the file is under budget, re-measure and delete this record with the final number
in the change history.
