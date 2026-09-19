# complexity: run_start_goal_without_output_dir

**Location:** `packages/tddy-core/src/presenter/workflow_runner.rs:356` — `run_start_goal_without_output_dir`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **275 lines** · **nesting depth 6** · 11 parameters · 30 branch/match lines · 8 early exits
**Thresholds breached:** length 275 > 60; nesting 6 > 4; parameters 11 > 5 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 275 | 6 | 30 | 8 | first detection |

## What the tool found

The body is **275 lines**, 4.6x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **30 branch or match lines** and **8 early exits**
(`return` / `?`). Its file is 1015 lines total, 1015 of them production, with **no `#[cfg(test)]` block**, across 0 functions.

**How this was found.** `/jev-restructuring` ranked it 43 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 275 > 60; nesting 6 > 4; parameters 11 > 5 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
