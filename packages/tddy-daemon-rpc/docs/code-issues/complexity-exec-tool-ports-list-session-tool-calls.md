# complexity: list_session_tool_calls

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/ports.rs:261` — `list_session_tool_calls` (now on `ExecToolRpcHandler`)
**Moved:** 2026-09-23 by #520 (#carve 11) — from `packages/tddy-session-lifecycle/src/connection_service/svc_exec_tool_ports.rs:232`; body unchanged except `self.x` → the handler's fields and components, which rustfmt re-wraps (94 → 101 lines)
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **94 lines** · **nesting depth 5** · 1 parameters · 2 branch/match lines · 9 early exits
**Thresholds breached:** length 94 > 60; nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 94 | 5 | 2 | 9 | first detection |
| 2026-09-23 | 101 | 5 | 2 | 9 | moved to `tddy-daemon-rpc`; length from re-wrapped field paths, not new logic |
| 2026-09-23 | 101 | 5 | 2 | 9 | re-measured at the #520 wrap: unchanged since the move; still at `exec_tool/ports.rs:261`, nesting and early exits identical to the lifecycle original |
| 2026-09-24 | 101 | 5 | 2 | 9 | touched by #509 (`#keyring` 2/9) and **unchanged by it**: `let os_user = self` → `&self` (the live `users:` holder), same line count, nesting and exits. Hand structural scan (fn line to closing brace; nesting by indentation; `return`/`?` count), identical method on the merge-base with `origin/master` (`4e7157d2`) and HEAD |

## What the tool found

The body is **94 lines**, 1.6x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **2 branch or match lines** and **9 early exits**
(`return` / `?`). Its file is 326 lines total, 326 of them production, with **no `#[cfg(test)]` block**, across 4 functions.

**How this was found.** `/jev-restructuring` ranked it 80 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 94 > 60; nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
