# complexity: pull_base_into_node_branch

**Location:** `packages/tddy-pr-stack/src/stack_ops/pull_base.rs:84` — `pull_base_into_node_branch`
**Moved:** 2026-09-22 by #496 (`#carve` 10/11) from `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` — verbatim apart from `tddy_core::worktree::` → `tddy_git::` path rewrites; the finding moved with the code, it did not close
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **159 lines** · **nesting depth 4** · 7 parameters · 10 branch/match lines · 15 early exits
**Thresholds breached:** length 159 > 60; parameters 7 > 5 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 159 | 4 | 10 | 15 | first detection |
| 2026-09-22 | 158 | 4 | 10 | 15 | moved to `tddy-pr-stack` by #496 — −1 line is a rustfmt rewrap of a shortened path, no logic change |

## What the tool found

The body is **159 lines**, 2.6x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **10 branch or match lines** and **15 early exits**
(`return` / `?`). At detection its file (`pr_stack/mod.rs`) was 3231 lines total, 1968 of them production, across 26 functions; since #496 its home `stack_ops/pull_base.rs` is 267 production lines.

**How this was found.** `/jev-restructuring` ranked it 63 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 159 > 60; parameters 7 > 5 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
