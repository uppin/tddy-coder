# complexity: start_sandboxed_claude_cli_session

**Location:** `packages/tddy-agent-launch/src/svc_start_sandboxed_claude_cli_session.rs:97` — `start_sandboxed_claude_cli_session`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **615 lines** · **nesting depth 5** · 1 parameters · 17 branch/match lines · 32 early exits
**CRAP:** **CRAP 2862** · complexity 53 · rank 2/50 in this crate · **never executed by any test**
**Thresholds breached:** length 615 > 60; nesting 5 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — narrowed 2026-09-24 by #524 (615 → 342) — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person
**Moved:** 2026-10-09 by `#carve` 21/21 (#536), engine move into `tddy-agent-launch`, from `packages/tddy-session-lifecycle/src/connection_service/svc_start_sandboxed_claude_cli_session.rs:96`

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 615 | 5 | 17 | 32 | first detection |
| 2026-09-24 | 342 | — | — | — | #524: plans `09b`, `09c`, `13` (the helpers into `jail_*` modules) and `18`, and DRY #5–#7 (615 → 342). What is left: the 23-parameter signature (~50 lines), three early returns and the two parameter-struct literals; the length goes with DRY #1's request struct, which waits on coverage |
| 2026-10-04 | — | — | — | — | touched by #573 (`#live-plan` 14/15): one added line, `env.extend(self.restructure_tools_env())`, beside the existing `lsp_tools_env` one (file 493 → 494 lines). Metrics not re-derived |
| 2026-10-05 | 342 | — | — | — | touched by the same-crate moves and **unchanged by them**: the file gained the `mod jail_env_builders;` line when that module was re-parented under it (494 to 495 production lines); the function is 342 lines at `origin/master` and at HEAD (fn line to closing brace), still at `:96`. Nesting, branches and exits not re-derived |
| 2026-10-05 | 343 | — | — | — | touched by #532 (`#carve` 17/21): +1 (342 → 343, fn line to closing brace) — the seed-clone claim goes through `.agent_roster()`. No control flow added; nesting, branches and exits not re-derived |
| 2026-10-07 | 343 | — | — | — | touched by #534 (`#carve` 19/21): converted onto `LaunchSessions` by an `impl` header change, the roster call through `self.agent_roster`, and imports re-pointed (A4). **Unchanged: 343** (fn line to closing brace, `:94`–`:436`, the same count at the base `7abe4a74`); file 496 → 494 production lines. No branch added; still never executed on macOS (its suites sit in the known-red 22 — the sandbox RPC bridge is never installed), so its preservation rests on compiling, the token-only edit rule and Linux CI. Nesting, branches and exits not re-derived |
| 2026-10-09 | 344 | — | — | — | `#carve` 21/21 (#536): **moved whole** from lifecycle into `tddy-agent-launch` (the receiver lifecycle's wiring crate now consumes). Length by brace matching (fn line to closing brace) is identical on `origin/master` (`468b368f9`, the old path) and on HEAD, so the move changed no length, nesting or branch; the new location is the only difference. Re-measured structurally only: complexity, CRAP and coverage were **not** re-derived (no `analyze coverage` run), so those figures stay the earlier ones. Still open, unclaimed |

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
