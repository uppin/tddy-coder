# complexity: start_session_core

**Location:** `packages/tddy-agent-launch/src/svc_start_session_core.rs:62` — `start_session_on_request` (was `start_session_core`, renamed by #669)
**Category:** complexity
**Detected:** 2026-09-18 — targeted by `/jev-restructuring` sweep, measured by structural scan
**Metrics:** **842 lines** · **nesting depth 6** · 1 parameters · 40 branch/match lines · 54 early exits
**CRAP:** 2026-10-24, from `tddy-tools analyze`: complexity **49**. **CRAP 2,450**, rank 1/50, when only this crate's own tests run: none of them enters it. **CRAP 49** once the session-lifecycle project-home suites are added, because they enter it. Those two suites execute **25%** of its regions (100/393). The 2026-09-18 figure was CRAP 80, complexity 80, fully covered, measured in lifecycle where its tests then lived
**Thresholds breached:** length 842 > 60; nesting 6 > 4 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — regressed 2026-10-24 (381 → 391, +10 from #741); regressed 2026-10-04 (358 → 373, +15 from #571); narrowed 2026-09-24 by #524 (857 → 358); what is left is its early-return guards, which the engine refuses to extract (E4, plan `19`) — **unclaimed**, **low priority**: fully covered, so this is a readability cost, not a risk
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable; the finding itself has not been read by a person
**Moved:** 2026-10-09 by `#carve` 21/21 (#536), engine move into `tddy-agent-launch`, from `packages/tddy-session-lifecycle/src/connection_service/svc_start_session_core.rs:62`

## Measurement history

| Run | Lines | Nesting | Branches | Early exits | Note |
|---|---|---|---|---|---|
| 2026-09-18 | 842 | 6 | 40 | 54 | first detection |
| 2026-09-23 | 857 | 6 | — | — | 854 on master before #520 (+12 since detection, unrecorded); +3 from #520 (`#carve` 11/12) — rustfmt re-wraps the peer-roster and common-room reads that moved behind `self.peer_routing`. No control flow added: nesting, and `return`/`?` count, identical to master; branches not re-derived |
| 2026-09-24 | 857 | 6 | — | — | touched by #509 (`#keyring` 2/9) and **unchanged by it**: `let os_user = self` → `&self` (the live `users:` holder), same line count; nesting and `return`/`?` count identical on the merge-base with `origin/master` (`4e7157d2`) and HEAD; branches not re-derived |
| 2026-09-24 | 358 | — | — | — | #524: plans `10a` and `10b` (15 extract-methods between the returns) and DRY #2, #4, #8 (857 → 358). Plan `19` authored the four branch bodies and the agent-allowlist check as extract-methods; each was refused for its early returns (E4) |
| 2026-10-04 | 373 | — | — | — | #571 (`#live-plan` 12/15): 358 → 373 (+15), by brace matching from the `fn start_session_core` line on `origin/feature/live-plan/session-lsp-tools` (which reproduces the 358 above) and on HEAD. Added: the `begin_phase` / `end_phase` calls around the workspace start's seed and semantic index, the `announce_worktree_ready` call after each of the three reporting starts, and the `sessions_base` / `session_id` captures the claude-cli and cursor-cli branches take before their `.await?`. Nesting, branches and exits not re-derived. Not split: the remaining seams are the early-return guards the engine refuses (E4) |
| 2026-10-05 | 374 | — | — | — | touched by #532 (`#carve` 17/21): +1 (373 → 374, fn line to closing brace) — rustfmt wraps `.agent_roster().unwind_seeded_roster(…)` over one more line. No control flow added; nesting, branches and exits not re-derived |
| 2026-10-07 | 377 | — | — | — | touched by `#carve` 18/21 (`SplitSessions`): +3 (374 → 377, fn line to closing brace) from three `self.split_sessions()` re-points where the launch topic calls the split topic through its handle. No control flow added. Measured by brace matching on `HEAD` vs `origin/master` |
| 2026-10-08 | 386 | — | — | — | `#carve` 20/21 (launch start): +5 against `origin/master` (381 -> 386, fn line to closing brace). Re-pointed to `impl LaunchSessions`; rustfmt wraps the three roster calls (`self.agent_roster.agent_def_for_spawn(…)`, `seeded_roster_records(…)`) and the `DaemonSeedCloneClaimant { service: … }` literal over more lines. No control flow added. **Regressed, kept open**: still over 60 lines |
| 2026-10-09 | 386 | — | — | — | `#carve` 21/21 (#536): **moved whole** from lifecycle into `tddy-agent-launch` (the receiver lifecycle's wiring crate now consumes). Length by brace matching (fn line to closing brace) is identical on `origin/master` (`468b368f9`, the old path) and on HEAD, so the move changed no length, nesting or branch; the new location is the only difference. Re-measured structurally only: complexity, CRAP and coverage were **not** re-derived (no `analyze coverage` run), so those figures stay the earlier ones. Still open, unclaimed |
| 2026-10-22 | 393 | — | — | — | touched by #669 (`#provider-route` 8/9) and **unchanged by it**: 393 on `origin/feature/provider-route/opencode-acp` (`1b488545a`) and on HEAD, by brace matching (fn line to closing brace). The function is renamed `start_session_on_request` (`pub(crate)`); `start_session_core` is now a two-statement wrapper in `harness_profile_request.rs` that applies a harness profile first, so the profile step added no line here. Nesting, branches and exits not re-derived |
| 2026-10-24 | 384 | 6 | — | — | **improved** (393 → 384, −9, by brace matching at `c1df8e142` and `4348f160a`) by `fix-main-agent-harness-se-e79c8de0`. That branch replaced the two inline `claude-cli` / `cursor-cli` empty-model `if`s with one call to `start_request_checks::refuse_a_cli_start_without_a_model`, and made `managed_recipe_for` `pub(crate)` so `refuse_a_claude_cli_start_up_front` can reuse it. That is one branch fewer here and no control flow added. `/analyze-code-issues` re-run: complexity 49. Coverage depends on which tests run: this crate's own 182 tests (CRAP 2,450, not entered) are the cross-crate gap. Lifecycle `ensure_project_home_session_acceptance` + `project_home_idle_stop_acceptance` enter it (CRAP 49, 25% of regions). Nesting is the brace-depth scan's 6, unchanged |
| 2026-10-24 | 391 | — | — | — | **regressed** by `#start-progress` 8/8 (#741): 381 on `origin/feature/start-progress/progress-ui` → 391 at HEAD (+10, fn line to closing brace). Added: the `cancel` parameter, rustfmt wrapping the split start's call over six lines once it takes `cancel`, and one `start_task::refuse_a_cancelled_start(cancel)?` after the clone so a cancelled start cuts no worktree. The cancellation helpers live in `start_task.rs`, not here. **Deferred with the developer's consent, #start-progress 8/8.** Nesting, branches and exits not re-derived |

## What the tool found

The body is **842 lines**, 14.0x the 60-line ceiling at which `/analyze-clean-code` says a function must be refactored. It is the **only function in its file**, so the file's 896 production lines are this function and nothing else — there is nothing else to move out.

The function carries **40 branch or match lines** and **54 early exits**
(`return` / `?`). Its file is 896 lines total, 896 of them production, with **no `#[cfg(test)]` block**.

**How this was found.** `/jev-restructuring` ranked it 1 of 3,503 production units by
semantic shape (Jev classified it `tangled_dispatch`). That ranking is **targeting only** and appears
in no metric above — every number in this record comes from a structural scan and can be re-derived
without an API call.

## Why it matters here

A function that is the whole of its file has no seam a reader can use to skip past what their change does not touch.
With no unit test in the file, nothing catches a behaviour change made while restructuring it.

## What would close it

Bring it under the `/analyze-clean-code` thresholds — length 842 > 60; nesting 6 > 4 — by `extract_method`
along the branch structure. Anchor with `tddy-tools restructure anchors`, never by hand, then prove
the seam with `restructure check --deep` against a warm index (`./run-index-daemon`).

⚠ **Re-measure before acting.** This record was generated in a batch of 100 from one sweep. Confirm
the numbers still hold and that the finding is real before spending a PR on it — an unverified
finding is a lead, not an issue.

## Measurement update 2026-10-17 — clone passphrase prompt

**Regressed by one line:** 388 → 389 lines by brace matching (nesting 6, unchanged). The call to `provision_project_for_start` gained its
`progress` argument and wraps to a second line; the keep-alive it enables lives in `start_keepalive.rs`. Not claimed by any PR.

## Measurement update 2026-10-22 — opencode-cli start

**Regressed by five lines:** 386 → 391 by brace matching against `origin/feature/provider-route/claude-route`
(`#provider-route` 3/9, #664). One more session-type branch, `opencode-cli`, as a single call to
`LaunchSessions::start_opencode_cli` in `cli_branch_starts.rs` — its refusals, prelude, roster and spawn
live there, not in this function. Nesting unchanged. Not claimed by any PR.

## Measurement update 2026-10-22 — CLI harness registry dispatch

**Unchanged** by `#provider-route` 6/9 (#667), by brace matching against
`origin/feature/provider-route/default-route`: the `opencode-cli` branch became a lookup in the CLI
harness registry (`cli_harness(session_type)` → `LaunchSessions::start_cli_harness`), so `muse-cli` and
any later harness start through it without another branch. Same line count (one comment line, three
lines of `if`), same nesting. Not claimed by any PR.

## Verified by hand

2026-10-24 (`/analyze-code-issues` on `fix-main-agent-harness-se-e79c8de0`): I read the diff to
this function. The two removed `if`s are replaced by one `refuse_a_cli_start_without_a_model(&req)?`
with the same messages. That function is now shared with the project-home up-front check, which
formats the message from the trimmed session type. The finding is still real: 384 lines, early-return
guards the engine refuses (E4). **Analyzer blind spot, found on this run:** `tddy-tools analyze
coverage` instruments only the crate under analysis. The body of an `async fn`, its `{closure#0}`,
was codegen'd in the *calling* crate (`…set_up_main0Cs…_22tddy_session_lifecycle` in the llvm
profile). So even a cross-crate run that instruments only `tddy_agent_launch` records the call (the
outer fn's count) but **0 regions of the body**. The 25% above comes from a capture with every
`tddy_*` crate instrumented.

