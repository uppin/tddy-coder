# oversized-file: roster/registry.rs

**Location:** `packages/tddy-discovery/src/roster/registry.rs`
**Category:** oversized-file
**Detected:** 2026-10-24 — `/pr-wrap` step 3.5 on `#coagents` 2/8 (#721), production lines counted to the first `#[cfg(test)]`
**Metrics:** **805 production lines** · budget 500
**Thresholds breached:** length 805 > 500
**Restructure:** `extract_module` — `/code-restructuring` territory
**Status:** Open — unclaimed; split deferred because `#coagents` 5/8 (`clone-placement`) and 7/8 (`headless-driver`) in the same stack also edit this file
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable

## Measurement history

| Run | Production lines | Note |
|---|---:|---|
| 2026-10-24 | 802 → 805 | first detection; **+3** in #721: a seeded entry is stamped `kind: MODEL`, `harness: ""` (coding agents are never seeded into the jail). Already over 500 before this PR |
| 2026-10-25 | 805 → 806 | **+1** in PR #724 (`#coagents` 5/8): a seeded entry carries `placement`. Not split: stack-shared |
| 2026-10-25 | 806 → 808 | **+2** in PR #726 (`#coagents` 7/8): a seeded entry carries empty `transport` and `permission_policy`. Not split: stack-shared; deferred to the `#coagents` sweep node by the developer (2026-10-25) |

## What would close it

Split the registry by concern into sibling modules under `roster/` (the entry/record conversions,
the revisioned store, the stream fan-out) until each is under 500, after `#coagents` lands so the
stack's diffs do not conflict.
