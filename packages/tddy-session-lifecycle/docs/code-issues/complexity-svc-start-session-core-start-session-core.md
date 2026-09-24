# complexity: start_session_core

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_start_session_core.rs:54` — `start_session_core`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **842 lines** · **nesting depth 6** · 1 parameters · 40 branch/match lines · 54 early exits
**CRAP:** **CRAP 80** · complexity 80 · rank 46/50 in this crate · **fully covered** (CRAP == complexity means coverage 1.0)
**Thresholds breached:** length 842 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-09-23 (842 → 857 since detection; +3 of it from #520, re-wrapped field reads) — **unclaimed**, **low priority**: fully covered, so this is a readability cost, not a risk
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 842 | 6 | 40 | 54 | first detection |
| 2026-09-23 | 857 | 6 | — | — | 854 on master before #520 (+12 since detection, unrecorded); +3 from #520 (`#carve` 11/12) — rustfmt re-wraps the peer-roster and common-room reads that moved behind `self.peer_routing`. No control flow added: nesting, and `return`/`?` count, identical to master; branches not re-derived |
| 2026-09-24 | 857 | 6 | — | — | touched by #509 (`#keyring` 2/9) and **unchanged by it**: `let os_user = self` → `&self` (the live `users:` holder), same line count; nesting and `return`/`?` count identical on the merge-base with `origin/master` (`4e7157d2`) and HEAD; branches not re-derived |

## What the tool found

The body is **842 lines**, 14.0x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored. It is the **only function in its file**, so the file's 896 production lines are this function and nothing else — there is nothing else to move out.

The function carries **40 branch or match lines** and **54 early exits**
(`return` / `?`). Its file is 896 lines total, 896 of them production, with **no `#[cfg(test)]` block**.

**How this was found.** `/jev-restructuring` ranked it 1 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

A function that is the whole of its file has no seam a reader can use to skip past what their change does not touch.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 842 > 60; nesting 6 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
