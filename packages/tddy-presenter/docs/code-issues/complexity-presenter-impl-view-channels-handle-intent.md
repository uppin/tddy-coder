# complexity: handle_intent

**Location:** `packages/tddy-presenter/src/presenter/presenter_impl/view_channels.rs:31` — `handle_intent`
**Moved:** 2026-09-23 — from `packages/tddy-core/src/presenter/presenter_impl/view_channels.rs:31` by `#carve` 12/12 (PR #522), which carved `tddy-core` into a wiring point; the body moved unchanged, so the metrics below still hold
**Moved:** 2026-09-22 from `presenter_impl.rs:520` — #495 (`#carve` 8/9) split the body into per-group handlers across the partition modules; `handle_intent` is now the dispatcher.
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **379 lines** · **nesting depth 8** · 1 parameters · 46 branch/match lines · 9 early exits
**Thresholds breached:** length 379 > 60; nesting 8 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **partially fixed** 2026-09-22 by #495 (dispatcher is 42 lines / nesting 2; the remainder lives in two extracted handlers — see What would close it) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 379 | 8 | 46 | 9 | first detection |
| 2026-09-19 | 379 | 8 | 46 | 9 | #491 rewrote every field access in this body (`self.<field>` → `self.<group>.<field>`). Nesting and branch structure **unchanged**; lines unchanged. The finding stands untouched. |
| 2026-09-22 | 42 | 2 | — | 1 | #495 split the body: each match arm moved verbatim into a handler in the partition owning its state; `handle_intent` is now a flat dispatcher. **The unit is clean.** Two handlers extracted from it still breach: `continue_with_agent` (`workflow_run.rs:278`, 60 lines, nesting 6) and `resume_from_error` (`workflow_run.rs:340`, 61 lines, nesting 4). Brace-depth scan. |
| 2026-09-23 | 42 | 2 | — | 1 | **unchanged** — measured in the new home at wrap: the body is line-for-line identical to its `tddy-core` origin on `master` (`#carve` 12/14, #522 moved it with `git mv`) |

## What the tool found

The body is **379 lines**, 6.3x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **46 branch or match lines** and **9 early exits**
(`return` / `?`). Its file is 2690 lines total, 1789 of them production, across 47 functions.

**How this was found.** `/jev-restructuring` ranked it 2 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

The dispatcher itself is under the thresholds. What remains is the arm complexity #495 moved verbatim into two handlers in `packages/tddy-core/src/presenter/presenter_impl/workflow_run.rs`:

- `continue_with_agent` (:278) — nesting 6 > 4 (60 lines, at the ceiling)
- `resume_from_error` (:340) — length 61 > 60

Bring both under the `/analyze-clean-code` thresholds by `extract_method` along their branch structure; then this record closes. Anchor with `tddy-tools restructure anchors`, never by hand, then prove the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).


⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

The file-level [`god-object-presenter`] record was **closed by #495** on 2026-09-22: the 46-method `impl` became nine `impl Presenter` blocks across seven files (three in the parent, one per partition) along `#carve` 4/9's state boundaries (final measurement in [`docs/dev/changesets/2026-09-22-carve-presenter-split.md`](../../../../docs/dev/changesets/2026-09-22-carve-presenter-split.md)). This record is about the unit and stands on its own.
