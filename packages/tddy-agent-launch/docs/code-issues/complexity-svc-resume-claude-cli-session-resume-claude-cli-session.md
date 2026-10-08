# complexity: resume_claude_cli_session

**Location:** `packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs:21` — `resume_claude_cli_session`
**Category:** complexity
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **119 lines** · nesting 4 · **9 parameters** · budget 60 / 4 / 5
**Thresholds breached:** length 119 > 60; parameters 9 > 5
**Restructure:** an options struct for the parameters; `extract_method --variant module` for the length
**Status:** Open — **regressed 2026-09-19**
**Moved:** 2026-10-09 by `#carve` 21/21 (#536), engine move into `tddy-agent-launch`, from `packages/tddy-session-lifecycle/src/connection_service/svc_resume_claude_cli_session.rs:21`

## Measurement history

| Run | Lines | Params | Note |
|---|---|---|---|
| 2026-09-19 | 119 | 9 | 112 → 119 and **8 → 9 parameters** in PR #518 |
| 2026-09-24 | 119 | 9 | re-measured for #524 (2026-09-24): unchanged, and not touched by it |
| 2026-10-05 | 119 | 9 | touched by the same-crate moves and **unchanged by them**: `split_pairing` is named through `peer_session_answer`, one line re-pointed; 119 lines and 9 parameters at `origin/master` and at HEAD (fn line to closing brace) |
| 2026-10-07 | 120 | 9 | touched by `#carve` 18/21: the T4 halves (`resume_split_wiring`, `split_roster_from_codebase_host`) were extracted to `svc_resume_claude_cli_session/svc_resume_split_wiring.rs`; this function gained **1** line (119 → 120, rustfmt wrap). **Parameters not re-counted here**: a brace-matching count on `HEAD` reads 8 against this record's 9 (it may count differently), so the 9 is carried unchanged and the discrepancy is left for the next measurement |
| 2026-10-08 | 120 | 7 incl. `&self` | `#carve` 20/21: converted to `impl LaunchSessions` (Recipe B). 121 -> 120 against `origin/master` (fn line to closing brace); the signature (`&self` + 6) is identical at `origin/master` and `HEAD`, so **the converted node did not add parameters**. The record's 9 is not reproduced by a parameter count of the signature (7 incl. `&self`); the count method is unclear. Still over the 60-line and 5-parameter budgets: **kept open, unchanged** |
| 2026-10-09 | unchanged | — | `#carve` 21/21 (#536): **moved whole** from lifecycle into `tddy-agent-launch` (the receiver lifecycle's wiring crate now consumes). Length by brace matching (fn line to closing brace) is identical on `origin/master` (`468b368f9`, the old path) and on HEAD, so the move changed no length, nesting or branch; the new location is the only difference. Re-measured structurally only: complexity, CRAP and coverage were **not** re-derived (no `analyze coverage` run), so those figures stay the earlier ones. Still open, unclaimed |

## What grew it

PR #518 threaded `sessions_base` through so a resumed sandboxed-codebase session could re-provision
its jail, and added the co-located branch selecting `resume_colocated_jail_wiring` over
`resume_split_wiring`. The co-located branch exists because `resume_split_wiring` reaches
`SplitLiveKitRoom::from_config`, which refuses on a daemon with no LiveKit — exactly the daemon this
placement is defined by.

## What would close it

A `ResumeClaudeCli { … }` options struct is the first move, and it is shared with
`resume_split_wiring` (7 parameters) and `resume_colocated_jail_wiring` (6) — all three take
overlapping slices of the same session state, so one struct closes three findings.
