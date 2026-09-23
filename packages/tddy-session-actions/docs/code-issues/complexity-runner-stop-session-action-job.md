# complexity: stop_session_action_job

**Location:** `packages/tddy-session-actions/src/session_action_jobs/runner.rs:343` — `stop_session_action_job`
**Moved:** 2026-09-23 — from `packages/tddy-core/src/session_action_jobs/runner.rs:343` by `#carve` 12/12 (PR #522), which carved `tddy-core` into a wiring point; the body moved unchanged, so the metrics below still hold
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **40 lines** · **nesting depth 5** · 1 parameters · 5 branch/match lines · 5 early exits
**Thresholds breached:** nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 40 | 5 | 5 | 5 | first detection |

## What the tool found

The body reaches **nesting depth 5**, 1.2x the depth at which `/analyze-clean-code` says a function must be refactored. Depth, not length, is the dominant defect here: at depth 5 a reader tracking one branch is holding 4 enclosing conditions that the indentation alone no longer makes visible.

The function carries **5 branch or match lines** and **5 early exits**
(`return` / `?`). Its file is 382 lines total, 382 of them production, with **no `#[cfg(test)]` block**, across 17 functions.

**How this was found.** `/jev-restructuring` ranked it 65 of 3,503 production units by
semantic shape (Jev classified it `none`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
