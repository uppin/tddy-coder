# complexity: build

**Location:** `packages/tddy-daemon/src/runtime.rs:563` — `build`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **806 lines** · **nesting depth 5** · 2 parameters · 18 branch/match lines · 10 early exits
**Thresholds breached:** length 806 > 60; nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-09-22 (806 → 833 lines since detection; +2 of it from #494) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 806 | 5 | 18 | 10 | first detection |
| 2026-09-22 | 833 | 5 | — | — | 831 on master before #494; +2 from #494 (`#carve` 8/11), the `SharedPresenterEventSink` cast at the `DaemonSessionHost::new` call. Nesting by indentation unchanged; branches and exits not re-derived |
| 2026-09-23 | 833 | 5 | — | — | touched by #520 (`#carve` 11/12) and **unchanged by it**, now at `runtime.rs:563`: `RpcHandlers::install(host)` added five lines and the four families' service and entry construction left for `RpcHandlers`, net zero. Nesting and `return`/`?` count identical to master; branches not re-derived |

## What the tool found

The body is **806 lines**, 13.4x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **18 branch or match lines** and **10 early exits**
(`return` / `?`). Its file is 1481 lines total, 1421 of them production, across 14 functions.

**How this was found.** `/jev-restructuring` ranked it 86 of 3,503 production units by
semantic shape (Jev classified it `god_function`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 806 > 60; nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
