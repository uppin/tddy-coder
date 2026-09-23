# complexity: capture_agent_activity

**Location:** `packages/tddy-presenter/src/presenter/presenter_impl/activity.rs:49` — `capture_agent_activity`
**Moved:** 2026-09-23 — from `packages/tddy-core/src/presenter/presenter_impl/activity.rs:49` by `#carve` 12/12 (PR #522), which carved `tddy-core` into a wiring point; the body moved unchanged, so the metrics below still hold
**Moved:** 2026-09-22 from `presenter_impl.rs:214` — #495 (`#carve` 8/9) partitioned `presenter_impl.rs`; the body moved **verbatim** (whitespace-identical).
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **101 lines** · **nesting depth 4** · 1 parameters · 5 branch/match lines · 2 early exits
**Thresholds breached:** length 101 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 101 | 4 | 5 | 2 | first detection |
| 2026-09-19 | 101 | 4 | 5 | 2 | #491 rewrote every field access in this body (`self.<field>` → `self.<group>.<field>`). Nesting and branch structure **unchanged**; lines unchanged. The finding stands untouched. |
| 2026-09-22 | 101 | 3¹ | 5 | 2 | Moved to `presenter_impl/activity.rs` by #495, body unchanged. **Unchanged** — the finding moved with it. ¹ brace-depth scan, reads one lower than the first-detection scan on this body; structure identical. |

## What the tool found

The body is **101 lines**, 1.7x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **5 branch or match lines** and **2 early exits**
(`return` / `?`). Its file is 2690 lines total, 1789 of them production, across 47 functions.

**How this was found.** `/jev-restructuring` ranked it 89 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 101 > 60 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

The file-level [`god-object-presenter`] record was **closed by #495** on 2026-09-22: the 46-method `impl` became nine `impl Presenter` blocks across seven files (three in the parent, one per partition) along `#carve` 4/9's state boundaries (final measurement in [`docs/dev/changesets/2026-09-22-carve-presenter-split.md`](../../../../docs/dev/changesets/2026-09-22-carve-presenter-split.md)). This record is about the unit and stands on its own.
