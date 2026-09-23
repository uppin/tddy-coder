# complexity: list_exec_tools

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/ports.rs:182` — `list_exec_tools` (now on `ExecToolRpcHandler`)
**Moved:** 2026-09-23 by #520 (#carve 11) — from `packages/tddy-session-lifecycle/src/connection_service/svc_exec_tool_ports.rs:158`; body unchanged except `self.x` → the handler's fields and components, which rustfmt re-wraps (73 → 78 lines)
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **73 lines** · **nesting depth 5** · 1 parameters · 2 branch/match lines · 7 early exits
**Thresholds breached:** length 73 > 60; nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 73 | 5 | 2 | 7 | first detection |
| 2026-09-23 | 78 | 5 | 2 | 7 | moved to `tddy-daemon-rpc`; length from re-wrapped field paths, not new logic |

## What the tool found

The body reaches **nesting depth 5**, 1.2x the depth at which `/analyze-clean-code` says a function must be refactored. Depth, not length, is the dominant defect here: at depth 5 a reader tracking one branch is holding 4 enclosing conditions that the indentation alone no longer makes visible.

The function carries **2 branch or match lines** and **7 early exits**
(`return` / `?`). Its file is 326 lines total, 326 of them production, with **no `#[cfg(test)]` block**, across 4 functions.

**How this was found.** `/jev-restructuring` ranked it 46 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 73 > 60; nesting 5 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
