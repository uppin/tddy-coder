# complexity: poll_workflow

**Location:** `packages/tddy-core/src/presenter/presenter_impl.rs:166` — `poll_workflow`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **235 lines** · **nesting depth 8** · 0 parameters · 20 branch/match lines · 1 early exits
**Thresholds breached:** length 235 > 60; nesting 8 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **partially fixed** 2026-09-22 by #495 (dispatcher is 32 lines / nesting 3; the remainder lives in one extracted handler) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 235 | 8 | 20 | 1 | first detection |
| 2026-09-19 | 236 | 8 | 20 | 1 | #491 rewrote every field access in this body (`self.<field>` → `self.<group>.<field>`). Nesting and branch structure **unchanged**; lines +1 (rustfmt rewrap). The finding stands untouched. |
| 2026-09-22 | 32 | 3 | — | 1 | #495 split the body: each event arm moved verbatim into an `on_*` handler in the partition owning its state; `poll_workflow` is now a dispatcher (still in the parent). **The unit is clean.** One extracted handler still breaches: `on_workflow_complete` (`workflow_run.rs:403`, 66 lines, nesting 4). Brace-depth scan (reads 7 on the pre-split body where the first scan read 8). |

## What the tool found

The body is **235 lines**, 3.9x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **20 branch or match lines** and **1 early exits**
(`return` / `?`). Its file is 2690 lines total, 1789 of them production, across 47 functions.

**How this was found.** `/jev-restructuring` ranked it 13 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

The dispatcher itself is under the thresholds. What remains is `on_workflow_complete` (`packages/tddy-core/src/presenter/presenter_impl/workflow_run.rs:403`) — length 66 > 60 — which #495 moved verbatim out of this body's `WorkflowComplete` arm. Bring it under the `/analyze-clean-code` thresholds by `extract_method`; then this record closes. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).


⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

The file-level [`god-object-presenter`] record was **closed by #495** on 2026-09-22: the 46-method `impl` became nine `impl Presenter` blocks across seven files (three in the parent, one per partition) along `#carve` 4/9's state boundaries (final measurement in [`docs/dev/changesets/2026-09-22-carve-presenter-split.md`](../../../../docs/dev/changesets/2026-09-22-carve-presenter-split.md)). This record is about the unit and stands on its own.
