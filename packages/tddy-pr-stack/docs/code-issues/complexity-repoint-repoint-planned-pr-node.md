# complexity: repoint_planned_pr_node

**Location:** `packages/tddy-pr-stack/src/stack_ops/repoint.rs:41` — `repoint_planned_pr_node`
**Moved:** 2026-09-22 by #496 (`#carve` 10/11) from `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` — verbatim apart from `tddy_core::worktree::` → `tddy_git::` path rewrites; the finding moved with the code, it did not close
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **79 lines** · **nesting depth 3** · 6 parameters · 3 branch/match lines · 3 early exits
**Thresholds breached:** length 79 > 60; parameters 6 > 5 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 79 | 3 | 3 | 3 | first detection |
| 2026-09-22 | 79 | 3 | 3 | 3 | moved to `tddy-pr-stack` by #496 — unchanged |

## What the tool found

The body is **79 lines**, 1.3x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **3 branch or match lines** and **3 early exits**
(`return` / `?`). At detection its file (`pr_stack/mod.rs`) was 3231 lines total, 1968 of them production, across 26 functions; since #496 its home `stack_ops/repoint.rs` is 228 production lines.

**How this was found.** `/jev-restructuring` ranked it 94 of 3,503 production units by
semantic shape (Jev classified it `none`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 79 > 60; parameters 6 > 5 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
