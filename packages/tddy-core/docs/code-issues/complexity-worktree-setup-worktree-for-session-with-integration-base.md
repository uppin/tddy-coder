# complexity: setup_worktree_for_session_with_integration_base

**Location:** `packages/tddy-core/src/worktree.rs:817` — `setup_worktree_for_session_with_integration_base`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **153 lines** · **nesting depth 6** · 3 parameters · 5 branch/match lines · 24 early exits
**Thresholds breached:** length 153 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 153 | 6 | 5 | 24 | first detection |

## What the tool found

The body is **153 lines**, 2.5x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **5 branch or match lines** and **24 early exits**
(`return` / `?`). Its file is 2375 lines total, 1607 of them production, across 4 functions.

**How this was found.** `/jev-restructuring` ranked it 3 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 153 > 60; nesting 6 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

`packages/tddy-core/src/worktree.rs` is already covered by [`squatting-git-plumbing-worktree`](../../../tddy-core/docs/code-issues/squatting-git-plumbing-worktree.md) — claimed by #492.
That record is about the file or type; this one is about the unit. Reconcile both together.
