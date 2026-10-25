# complexity: open_replay_ack_live

**Location:** `packages/tddy-terminal-rpc/src/bridge.rs:242` — `open_replay_ack_live`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **135 lines** · **nesting depth 6** · 5 parameters · 8 branch/match lines · 0 early exits
**Thresholds breached:** length 135 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-10-25 (135 → 141 lines in #760, 142 in #761) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 135 | 6 | 8 | 0 | first detection |
| 2026-10-25 | 141 | 6 | 9 | 1 | **worse** by 6 in PR #760 (`#term-snapshot` 1/4): the `SNAPSHOT` mode returns early into its own `open_snapshot` (the new branch is not inline), plus an `unreachable!` arm and the `Result` return. Splitting the TAIL / FROM_OFFSET branches deferred with the developer's consent (code issues may be deferred in this stack; `bridge.rs` is stack-shared with #761–#763). Brace-depth scan, not the original sweep tool |
| 2026-10-25 | 142 | 6 | 9 | 1 | +1 line in PR #761 (`#term-snapshot` 2/4): the destructuring names the new `resync` field. The resync itself is **not** here: it lives in `LiveOutput::resync` / `send_resync` as the changeset's prerequisite asked, so the open path did not grow. Splitting TAIL / FROM_OFFSET stays deferred (developer consent) |

## What the tool found

The body is **135 lines**, 2.2x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **8 branch or match lines** and **0 early exits**
(`return` / `?`). Its file is 561 lines total, 561 of them production, with **no `#[cfg(test)]` block**, across 6 functions.

**How this was found.** `/jev-restructuring` ranked it 37 of 3,503 production units by
semantic shape (Jev classified it `unsure`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 135 > 60; nesting 6 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
