# complexity: serve

**Location:** `packages/tddy-supervisor/src/server.rs:511` — `serve`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **95 lines** (2026-09-24; 90 at detection) · **nesting depth 5** · 2 parameters · 4 branch/match lines · 2 early exits
**Thresholds breached:** length 95 > 60; nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-09-24 (90 → 95 in #509, `#keyring` 2/9, transport stamping; deferred with consent) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 90 | 5 | 4 | 2 | first detection |
| 2026-09-24 | 95 | 5 | 4 | 2 | 90 on the merge-base with `origin/master` (`4e7157d2`) → 95 after #509 (`#keyring` 2/9): `StdioEndpoint::from_duplex` takes a fourth argument, `RequestTransport::UnixSocket`, and rustfmt wraps the call over six lines. No control flow added: nesting, branches and exits identical on base and HEAD. Decomposition deferred with the developer's consent (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`, which lists `server.rs` 703 → 708) |

## What the tool found

The body is **90 lines**, 1.5x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **4 branch or match lines** and **2 early exits**
(`return` / `?`). Its file is 862 lines total, 704 of them production, across 24 functions.

**How this was found.** `/jev-restructuring` ranked it 48 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 90 > 60; nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
