# complexity: resume_claude_cli_session

**Location:** `packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs:21` — `resume_claude_cli_session`
**Category:** complexity
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **119 lines** · nesting 4 · **9 parameters** · budget 60 / 4 / 5
**Thresholds breached:** length 119 > 60; parameters 9 > 5
**Restructure:** an options struct for the parameters; `extract_method --variant module` for the length
**Status:** Open — partially fixed 2026-10-24 (144 → 142 lines; still over the 60-line and 5-parameter budgets)
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
| 2026-10-22 | unchanged | — | touched by #663 (`#provider-route` 2/9): the resumed agent's environment (identity pairs, split wiring, the recorded provider route) is assembled in the new `LaunchSessions::resumed_agent_env`, so the body gained the route without growing — same length by brace matching on `origin/feature/provider-route/accounts` and HEAD; `tddy-tools analyze` measures complexity 13, CRAP 182 (never executed). Still open, unclaimed |
| 2026-10-23 | 144 | — | **worse** by 2 (142 → 144) in #700 (`#project-home` 5/7): the resumed agent is given its role's `--allowedTools` again (`split_args.extend(role_allowed_tools_args(meta.session_role.as_deref()))`). Measured by structural scan, fn line to closing brace on `origin/feature/project-home/home-session` (08cfc2a28) and HEAD; no coverage run. Still open, unclaimed |
| 2026-10-24 | 144 | — | **Not touched** by `fix-main-agent-harness-se-e79c8de0` (`c1df8e142..4348f160a`): length by brace matching is identical at both. `/analyze-code-issues` re-run: complexity 13. This crate's own tests never enter it (CRAP 182). The session-lifecycle `ensure_project_home_session_acceptance` + `project_home_idle_stop_acceptance` (27 tests), with every workspace crate instrumented enter it (CRAP 13) |
| 2026-10-24 | 142 | 8 incl. `&self` | **better** by 2 (144 → 142) in `2026-10-24-project-home-agent-tools`: the split-wiring `match` and the role's launch moved into `LaunchSessions::resumed_role(..).after_split(split)` (`project_home_launch.rs`), which also re-wires a resumed main to `tddy-tools`. `tddy-tools analyze`: complexity 13 (unchanged; a transient 14 mid-change was removed), CRAP 182. Fn line to closing brace on `origin/master` (d91ea44b7) and HEAD. Still open, unclaimed |
| 2026-10-26 | 141 | 8 incl. `&self` | **better** by 1 (142 → 141) in #766 (`#home-inbox` 1/8): the resumed agent's notify wiring is one call to the new `LaunchSessions::resumed_notify` (role and hook-token gates inside `notify_env::resumed_agent_notify_launch`), and the reply moved to `resumed_without_livekit`; the green phase had grown it to 159. Fn line to closing brace on `origin/feature/start-parity/spawn-conversation` (dfdfe8d93) and HEAD; no coverage run. Still open, unclaimed |

## What grew it

PR #518 threaded `sessions_base` through so a resumed sandboxed-codebase session could re-provision
its jail, and added the co-located branch selecting `resume_colocated_jail_wiring` over
`resume_split_wiring`. The co-located branch exists because `resume_split_wiring` reaches
`SplitAgentRoom::issued_by`, which refuses on a daemon with no room integration — exactly the daemon this
placement is defined by.

## What would close it

A `ResumeClaudeCli { … }` options struct is the first move, and it is shared with
`resume_split_wiring` (7 parameters) and `resume_colocated_jail_wiring` (6) — all three take
overlapping slices of the same session state, so one struct closes three findings.
