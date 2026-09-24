# complexity: facade_lines

**Location:** `packages/tddy-code-restructuring/src/backends/rust.rs:3522` — `facade_lines`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **47 lines** · **nesting depth 5** · 3 parameters · 5 branch/match lines · 1 early exits
**Thresholds breached:** nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 47 | 5 | 5 | 1 | first detection |
| 2026-09-24 | 47 | 5 | 5 | 1 | unchanged; #527 moved it from line 3978 to 3522 by taking code out of `rust.rs` above it, and did not touch the function |

## What the tool found

The body reaches **nesting depth 5**, 1.2x the depth at which `/analyze-clean-code` says a function must be refactored. Depth, not length, is the dominant defect here: at depth 5 a reader tracking one branch is holding 4 enclosing conditions that the indentation alone no longer makes visible.

The function carries **5 branch or match lines** and **1 early exits**
(`return` / `?`). Its file is 7028 lines total, 4758 of them production, across 66 functions.

**How this was found.** `/jev-restructuring` ranked it 81 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
