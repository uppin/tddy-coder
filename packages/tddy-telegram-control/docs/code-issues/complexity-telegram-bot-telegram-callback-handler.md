# complexity: telegram_callback_handler

**Location:** `packages/tddy-telegram-control/src/telegram_bot.rs:368` — `telegram_callback_handler`
**Moved:** 2026-09-22 from `tddy-session-lifecycle` by #494 (`#carve` 8/11) — the code moved unchanged, so the metrics still hold; a crate-relative rank above was measured in `tddy-session-lifecycle`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **421 lines** · **nesting depth 7** · 3 parameters · 46 branch/match lines · 54 early exits
**CRAP:** **CRAP 7832** · complexity 88 · rank 1/50 in this crate · **never executed by any test**
**Thresholds breached:** length 421 > 60; nesting 7 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 421 | 7 | 46 | 54 | first detection |

## What the tool found

The body is **421 lines**, 7.0x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **46 branch or match lines** and **54 early exits**
(`return` / `?`). Its file is 788 lines total, 788 of them production, with **no `#[cfg(test)]` block**, across 4 functions.

**How this was found.** `/jev-restructuring` ranked it 35 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 421 > 60; nesting 7 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Related

`packages/tddy-session-lifecycle/src/telegram_bot.rs` is already covered by [`crap-telegram-bot-handlers`](../../../tddy-session-lifecycle/docs/code-issues/crap-telegram-bot-handlers.md).
That record is about the file or type; this one is about the unit. Reconcile both together.
