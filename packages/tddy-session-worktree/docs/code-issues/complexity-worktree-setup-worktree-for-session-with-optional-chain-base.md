# complexity: setup_worktree_for_session_with_optional_chain_base

**Location:** `packages/tddy-session-worktree/src/worktree.rs:195` — `setup_worktree_for_session_with_optional_chain_base`
**Moved:** 2026-09-23 — from `packages/tddy-core/src/worktree.rs:195` by `#carve` 12/12 (PR #522), which carved `tddy-core` into a wiring point; the body moved unchanged, so the metrics below still hold
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **197 lines** · **nesting depth 6** · 3 parameters · 8 branch/match lines · 27 early exits
**Thresholds breached:** length 197 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 197 | 6 | 8 | 27 | first detection |
| 2026-09-22 | 197 | 6 | 8 | 27 | **unchanged** — #492 (`#carve` 6/11) moved the git plumbing around it to `tddy-git`; the body is byte-identical. Its file is now 428 production lines across these 4 functions, not 1,607 |

## What the tool found

The body is **197 lines**, 3.3x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **8 branch or match lines** and **27 early exits**
(`return` / `?`). Its file is 2375 lines total, 1607 of them production, across 4 functions.

**How this was found.** `/jev-restructuring` ranked it 4 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 197 > 60; nesting 6 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

`packages/tddy-core/src/worktree.rs` was covered by `squatting-git-plumbing-worktree` until #492
closed it on 2026-09-22 by moving the file's git plumbing to `tddy-git` (see
[the change entry](../changesets/2026-09-22-carve-git-plumbing.md)). This unit was not part of that
fix: it stayed in `tddy-core`, byte-identical, and this record is still open.
