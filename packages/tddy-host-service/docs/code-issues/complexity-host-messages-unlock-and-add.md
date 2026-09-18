# complexity: unlock_and_add

**Location:** `packages/tddy-host-service/src/host_messages.rs:118` — `unlock_and_add`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **80 lines** · **nesting depth 4** · 1 parameters · 5 branch/match lines · 3 early exits
**Thresholds breached:** length 80 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 80 | 4 | 5 | 3 | first detection |

## What the tool found

The body is **80 lines**, 1.3x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **5 branch or match lines** and **3 early exits**
(`return` / `?`). Its file is 243 lines total, 243 of them production, with **no `#[cfg(test)]` block**, across 0 functions.

**How this was found.** `/jev-restructuring` ranked it 73 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 80 > 60 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
