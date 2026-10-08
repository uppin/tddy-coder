# complexity: spawn_cursor_cli_session_inner

**Location:** `packages/tddy-agent-launch/src/cursor_cli_spawn.rs:28` — `spawn_cursor_cli_session_inner`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **337 lines** · **nesting depth 4** · 9 parameters · 10 branch/match lines · 20 early exits
**CRAP:** **CRAP 930** · complexity 30 · rank 6/50 in this crate · **never executed by any test**
**Thresholds breached:** length 337 > 60; parameters 9 > 5 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-10-04 (file crossed the 500-production-line budget in #571: 445 → 532). Narrowed 2026-09-24 by #524 (337 → 244 lines); the remaining seams are refused by the engine (T: `SpawnStackParent<'_>`; E4: early returns) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person
**Moved:** 2026-10-09 by `#carve` 21/21 (#536), engine move into `tddy-agent-launch`, from `packages/tddy-session-lifecycle/src/cursor_cli_spawn.rs:27`

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 337 | 4 | 10 | 20 | first detection |
| 2026-09-24 | 244 | — | — | — | #524: plans `03` (chat and resume out), `15` (6 extract-methods) and DRY #5–#8. What is left: the worktree-source `match` and its two early returns, the `stack_parent` calls the engine refuses (T), and the call sites. Nesting, branches and exits not re-derived; lines by the plan's fn-line-to-closing-brace count (337 at the merge-base) |
| 2026-10-04 | not re-derived (file: **532** production lines, 445 at base `c3567fde`) | — | — | — | #571 (`#live-plan` 12/15): +87 lines in the file — the `spawn_cursor_cli_session_reporting` wrapper and the phase reporting around `spawn_cursor_cli_session_inner`. File count: `awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}' packages/tddy-session-lifecycle/src/cursor_cli_spawn.rs` (no `#[cfg(test)]` block, so the whole file). The file now also holds helper fns (`cut_cursor_cli_worktree`, `prepare_cursor_cli_launch`, `spawn_cursor_cli_process`, …) beyond the one function. Growth deferred with the developer's consent: `docs/dev/todo/2026-10-04-cursor-cli-spawn-crossed-the-file-budget-in-indexing-indicators.md` |
| 2026-10-07 | unchanged | — | — | — | touched by #534 (`#carve` 19/21): imports only (3 lines re-pointed to their defining crates, A4). File **532 production lines before and after**; no function body changed. `spawn_cursor_cli_session_inner` is 72 lines (fn line to closing brace, `:26`–`:97`) and its sibling `spawn_cursor_cli_session_reporting` 251 (`:100`–`:350`); nesting, branches and exits not re-derived |
| 2026-10-09 | 75 | — | — | — | `#carve` 21/21 (#536): **moved whole** from lifecycle into `tddy-agent-launch` (the receiver lifecycle's wiring crate now consumes). Length by brace matching (fn line to closing brace) is identical on `origin/master` (`468b368f9`, the old path) and on HEAD, so the move changed no length, nesting or branch; the new location is the only difference. Re-measured structurally only: complexity, CRAP and coverage were **not** re-derived (no `analyze coverage` run), so those figures stay the earlier ones. Still open, unclaimed |

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
