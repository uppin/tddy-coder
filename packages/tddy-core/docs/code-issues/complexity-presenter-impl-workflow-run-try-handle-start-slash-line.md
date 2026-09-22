# complexity: try_handle_start_slash_line

**Location:** `packages/tddy-core/src/presenter/presenter_impl/workflow_run.rs:187` — `try_handle_start_slash_line`
**Moved:** 2026-09-22 from `presenter_impl.rs:1599` — #495 (`#carve` 8/9) partitioned `presenter_impl.rs`; the body moved **verbatim** (whitespace-identical).
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **49 lines** · **nesting depth 7** · 1 parameters · 5 branch/match lines · 3 early exits
**Thresholds breached:** nesting 7 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 49 | 7 | 5 | 3 | first detection |
| 2026-09-19 | 49 | 7 | 5 | 3 | #491 rewrote every field access in this body (`self.<field>` → `self.<group>.<field>`). Nesting and branch structure **unchanged**; lines unchanged. The finding stands untouched. |
| 2026-09-22 | 49 | 7 | 5 | 3 | Moved to `presenter_impl/workflow_run.rs` by #495, body unchanged. **Unchanged** — the finding moved with it. |

## What the tool found

The body reaches **nesting depth 7**, 1.8x the depth at which `/analyze-clean-code` says a function must be refactored. Depth, not length, is the dominant defect here: at depth 7 a reader tracking one branch is holding 6 enclosing conditions that the indentation alone no longer makes visible.

The function carries **5 branch or match lines** and **3 early exits**
(`return` / `?`). Its file is 2690 lines total, 1789 of them production, across 47 functions.

**How this was found.** `/jev-restructuring` ranked it 25 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — nesting 7 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

The file-level [`god-object-presenter`] record was **closed by #495** on 2026-09-22: the 46-method `impl` became nine `impl Presenter` blocks across seven files (three in the parent, one per partition) along `#carve` 4/9's state boundaries (final measurement in [`docs/dev/changesets/2026-09-22-carve-presenter-split.md`](../../../../docs/dev/changesets/2026-09-22-carve-presenter-split.md)). This record is about the unit and stands on its own.
