# complexity: start_sandboxed_claude_cli_session

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_start_sandboxed_claude_cli_session.rs:96` — `start_sandboxed_claude_cli_session`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **615 lines** · **nesting depth 5** · 1 parameters · 17 branch/match lines · 32 early exits
**CRAP:** **CRAP 2862** · complexity 53 · rank 2/50 in this crate · **never executed by any test**
**Thresholds breached:** length 615 > 60; nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — narrowed 2026-09-24 by #524 (615 → 342) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 615 | 5 | 17 | 32 | first detection |
| 2026-09-24 | 342 | — | — | — | #524: plans `09b`, `09c`, `13` (the helpers into `jail_*` modules) and `18`, and DRY #5–#7 (615 → 342). What is left: the 23-parameter signature (~50 lines), three early returns and the two parameter-struct literals; the length goes with DRY #1's request struct, which waits on coverage |

## What the tool found

The body is **615 lines**, 10.2x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored. It is the **only function in its file**, so the file's 661 production lines are this function and nothing else — there is nothing else to move out.

The function carries **17 branch or match lines** and **32 early exits**
(`return` / `?`). Its file is 661 lines total, 661 of them production, with **no `#[cfg(test)]` block**.

**How this was found.** `/jev-restructuring` ranked it 16 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

A function that is the whole of its file has no seam a reader can use to skip past what their change does not touch.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 615 > 60; nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
