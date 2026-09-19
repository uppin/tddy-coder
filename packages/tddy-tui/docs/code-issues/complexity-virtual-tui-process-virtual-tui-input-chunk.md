# complexity: process_virtual_tui_input_chunk

**Location:** `packages/tddy-tui/src/virtual_tui.rs:404` — `process_virtual_tui_input_chunk`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **131 lines** · **nesting depth 5** · 11 parameters · 17 branch/match lines · 0 early exits
**Thresholds breached:** length 131 > 60; nesting 5 > 4; parameters 11 > 5 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 131 | 5 | 17 | 0 | first detection |

## What the tool found

The signature takes **11 parameters**, over the 5 at which `/analyze-clean-code` calls for an options struct.

The function carries **17 branch or match lines** and **0 early exits**
(`return` / `?`). Its file is 1352 lines total, 902 of them production.

**How this was found.** `/jev-restructuring` ranked it 93 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

A function that is the whole of its file has no seam a reader can use to skip past what their change does not touch.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 131 > 60; nesting 5 > 4; parameters 11 > 5 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
