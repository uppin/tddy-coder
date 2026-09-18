# complexity: decode_utf8_prefix

**Location:** `packages/tddy-vm/src/serial_shell.rs:720` — `decode_utf8_prefix`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **29 lines** · **nesting depth 6** · 1 parameters · 3 branch/match lines · 2 early exits
**Thresholds breached:** nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 29 | 6 | 3 | 2 | first detection |

## What the tool found

The body reaches **nesting depth 6**, 1.5x the depth at which `/analyze-clean-code` says a function must be refactored. Depth, not length, is the dominant defect here: at depth 6 a reader tracking one branch is holding 5 enclosing conditions that the indentation alone no longer makes visible.

The function carries **3 branch or match lines** and **2 early exits**
(`return` / `?`). Its file is 748 lines total, 748 of them production, with **no `#[cfg(test)]` block**, across 27 functions.

**How this was found.** `/jev-restructuring` ranked it 36 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — nesting 6 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
