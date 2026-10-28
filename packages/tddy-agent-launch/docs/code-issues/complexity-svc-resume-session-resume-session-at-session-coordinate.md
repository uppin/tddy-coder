# complexity: resume_session_at_session_coordinate

**Location:** `packages/tddy-agent-launch/src/svc_resume_session.rs:31` — `resume_session_at_session_coordinate`
**Moved:** 2026-09-24 by #524 plan `08`, from `connection_service/session_coordinate_handlers.rs:271`; this record was `complexity-session-coordinate-handlers-resume-session-at-session-coordinate.md`
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **241 lines** · **nesting depth 6** · 1 parameters · 9 branch/match lines · 12 early exits
**CRAP:** **CRAP 462** · complexity 21 · rank 9/50 in this crate · **never executed by any test**
**Thresholds breached:** length 241 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-10-22 (173 → 181, `#provider-route` 3/9); narrowed 2026-09-24 by #524 (242 → 137); still over the 60-line budget — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person
**Moved:** 2026-10-09 by `#carve` 21/21 (#536), engine move into `tddy-agent-launch`, from `packages/tddy-session-lifecycle/src/connection_service/session_coordinate_handlers/svc_resume_session.rs:30`

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 241 | 6 | 9 | 12 | first detection |
| 2026-09-24 | 241 | 6 | 9 | 12 | touched by #509 (`#keyring` 2/9) and **unchanged by it**: `let os_user = self` → `&self` (the live `users:` holder), same line count, nesting, branches and exits; still at `session_coordinate_handlers.rs:271`. Hand structural scan (fn line to closing brace; nesting by indentation; `return`/`?` count), identical method on the merge-base with `origin/master` (`4e7157d2`) and HEAD. CRAP not re-run |
| 2026-09-24 | 137 | — | — | — | #524: moved by plan `08`; DRY #2 folded its inline `tddy-coder` spawn into `spawn_tddy_coder(ToolSpawnPlan)` (242 → 137 by the plan's count). Nesting, branches and exits not re-derived |
| 2026-10-07 | 138 | — | — | — | touched by `#carve` 18/21: +1 (137 → 138, rustfmt wrap of a re-pointed call). No control flow added |
| 2026-10-08 | 141 | — | — | — | `#carve` 20/21: converted to `impl LaunchSessions`; +3 against `origin/master` (138 -> 141, fn line to closing brace) from rustfmt re-wraps of re-pointed calls (`self.agent_roster.…`, `self.split_sessions.…`). No control flow added. Over the 60-line budget: **kept open** |
| 2026-10-09 | unchanged | — | — | — | `#carve` 21/21 (#536): **moved whole** from lifecycle into `tddy-agent-launch` (the receiver lifecycle's wiring crate now consumes). Length by brace matching (fn line to closing brace) is identical on `origin/master` (`468b368f9`, the old path) and on HEAD, so the move changed no length, nesting or branch; the new location is the only difference. Re-measured structurally only: complexity, CRAP and coverage were **not** re-derived (no `analyze coverage` run), so those figures stay the earlier ones. Still open, unclaimed |
| 2026-10-22 | 181 | — | — | — | `#provider-route` 3/9 (#664): 173 → 181 (+8, fn line to closing brace, against `origin/feature/provider-route/claude-route`) — one more session-type branch, the `opencode-cli` resume, as a three-line call to `LaunchSessions::resume_opencode_cli` (its body lives in `opencode_cli_spawn.rs`). **Regressed, kept open**; the session-type dispatch is the seam a descriptor-driven resume would remove |
| 2026-10-22 | unchanged | — | — | — | `#provider-route` 6/9 (#667), by brace matching against `origin/feature/provider-route/default-route`: the `opencode-cli` resume branch became a CLI-harness-registry lookup (`cli_harness(session_type)` → `LaunchSessions::resume_cli_harness`), the descriptor-driven resume this record named as the seam. Same line count and nesting; the function is still over budget. Kept open |
| 2026-10-24 | 202 | — | — | — | **Not touched** by `fix-main-agent-harness-se-e79c8de0` (`c1df8e142..4348f160a`): length by brace matching is identical at both. `/analyze-code-issues` re-run: complexity 23. This crate's own tests never enter it (CRAP 552). The session-lifecycle `ensure_project_home_session_acceptance` + `project_home_idle_stop_acceptance` (27 tests), with every workspace crate instrumented enter it (CRAP 23) |
| 2026-10-26 | 189 | — | — | — | **better** by 13 (202 → 189) in #766 (`#home-inbox` 1/8): the cursor-cli branch is one call to the new `LaunchSessions::resume_cursor_cli`, which also gives the resumed agent its notify environment (`chat::resumed_cursor_notify_env`); the green phase had grown it to 222. Fn line to closing brace on `origin/feature/start-parity/spawn-conversation` (dfdfe8d93) and HEAD; no coverage run. Still open, unclaimed |

## What the tool found

The body is **241 lines**, 4.0x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored.

The function carries **9 branch or match lines** and **12 early exits**
(`return` / `?`). Its file is 817 lines total, 817 of them production, with **no `#[cfg(test)]` block**, across 9 functions.

**How this was found.** `/jev-restructuring` ranked it 59 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

Within its file this is the body a change to this area has to be read in full to modify safely.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 241 > 60; nesting 6 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.
