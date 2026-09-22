# complexity: handle_elicitation_multi_select_shortcut

**Location:** `packages/tddy-telegram-control/src/telegram_session_control/elicitation.rs:273` — `handle_elicitation_multi_select_shortcut`
**Moved:** 2026-09-22 from `tddy-session-lifecycle` by #494 (`#carve` 8/11) — the code moved unchanged, so the metrics still hold; a crate-relative rank above was measured in `tddy-session-lifecycle`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **105 lines** · **nesting depth 4** · 4 parameters · 6 branch/match lines · 7 early exits
**CRAP:** **CRAP 272** · complexity 16 · rank 14/50 in this crate · **never executed by any test**
**Thresholds breached:** length 105 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 105 | 4 | 6 | 7 | first detection |

## What the tool found

The body is **105 lines**, 1.8x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **6 branch or match lines** and **7 early exits**
(`return` / `?`). Its file is 4476 lines total, 3981 of them production, across 70 functions.

**How this was found.** `/jev-restructuring` ranked it 77 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.


## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 105 > 60 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

`packages/tddy-session-lifecycle/src/telegram_session_control.rs` is already covered by [`oversized-file-telegram-session-control`](../../../tddy-session-lifecycle/docs/code-issues/oversized-file-telegram-session-control.md) — claimed by #494.
That record is about the file or type; this one is about the unit. Reconcile both together.
