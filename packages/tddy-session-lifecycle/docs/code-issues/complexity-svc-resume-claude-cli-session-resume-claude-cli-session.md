# complexity: resume_claude_cli_session

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_resume_claude_cli_session.rs` — `resume_claude_cli_session`
**Category:** complexity
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **119 lines** · nesting 4 · **9 parameters** · budget 60 / 4 / 5
**Thresholds breached:** length 119 > 60; parameters 9 > 5
**Restructure:** an options struct for the parameters; `extract_method --variant module` for the length
**Status:** Open — **regressed 2026-09-19**

## Measurement history

| Run | Lines | Params | Note |
|---|---|---|---|
| 2026-09-19 | 119 | 9 | 112 → 119 and **8 → 9 parameters** in PR #518 |
| 2026-09-24 | 119 | 9 | re-measured for #524 (2026-09-24): unchanged, and not touched by it |

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
