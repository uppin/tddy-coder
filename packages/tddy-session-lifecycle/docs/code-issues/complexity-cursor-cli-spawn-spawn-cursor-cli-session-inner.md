# complexity: spawn_cursor_cli_session_inner

**Location:** `packages/tddy-session-lifecycle/src/cursor_cli_spawn.rs:23` — `spawn_cursor_cli_session_inner`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **337 lines** · **nesting depth 4** · 9 parameters · 10 branch/match lines · 20 early exits
**CRAP:** **CRAP 930** · complexity 30 · rank 6/50 in this crate · **never executed by any test**
**Thresholds breached:** length 337 > 60; parameters 9 > 5 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — narrowed 2026-09-24 by #524 (337 → 244 lines); the remaining seams are refused by the engine (T: `SpawnStackParent<'_>`; E4: early returns) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 337 | 4 | 10 | 20 | first detection |
| 2026-09-24 | 244 | — | — | — | #524: plans `03` (chat and resume out), `15` (6 extract-methods) and DRY #5–#8. What is left: the worktree-source `match` and its two early returns, the `stack_parent` calls the engine refuses (T), and the call sites. Nesting, branches and exits not re-derived; lines by the plan's fn-line-to-closing-brace count (337 at the merge-base) |

## What the tool found

The body is **337 lines**, 5.6x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored. It is the **only function in its file**, so the file's 524 production lines are this function and nothing else — there is nothing else to move out.

The function carries **10 branch or match lines** and **20 early exits**
(`return` / `?`). Its file is 524 lines total, 524 of them production, with **no `#[cfg(test)]` block**.

**How this was found.** `/jev-restructuring` ranked it 66 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

A function that is the whole of its file has no seam a reader can use to skip past what their change does not touch.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 337 > 60; parameters 9 > 5 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
