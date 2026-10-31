# complexity: start_sandboxed_cli_harness_session

**Location:** `packages/tddy-agent-launch/src/svc_start_sandboxed_cli_harness_session.rs:90` — `LaunchSessions::start_sandboxed_cli_harness_session`
**Category:** complexity
**Detected:** 2026-10-22 by structural audit — `/pr-wrap` on PR #670 (`#provider-route` 9/9), which added the function
**Metrics:** **250 lines** (fn line to closing brace) · brace depth 3 · CRAP not measured (no `analyze coverage` run)
**Thresholds breached:** length 250 > 60 (`/analyze-clean-code`)
**Restructure:** required — `extract_method` × 4, `/code-restructuring` territory, blocked as below
**Status:** Open — **unclaimed**. Added by #670 and not closed in it: a `/green-yolo` stop condition, left for the developer
**Claimed by:** #797 — `#agent-tools 8/12`
**Lands after:** #796

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-10-22 | 277 | 3 | as #670's green phase wrote it: the sandboxed Cursor start's sequence (roster warm-up, worktree, seeded clones, jail dirs, runner argv, env, spawn, bridge, metadata) for `opencode-cli` / `muse-cli` |
| 2026-10-22 | 250 | 3 | same PR, at wrap: the engine extracted `bridge_and_register_cli_harness_jail` (−27) and moved the pure helpers to `cli_harness_jail.rs` (the file went from 560 to 390 production lines) |
| 2026-10-24 | 250 | — | **Not touched** by `fix-main-agent-harness-se-e79c8de0` (`c1df8e142..4348f160a`): length by brace matching is identical at both. `/analyze-code-issues` re-run: complexity 22, **CRAP 506**, never entered, either by this crate's own tests or by the session-lifecycle `ensure_project_home_session_acceptance` + `project_home_idle_stop_acceptance` (27 tests), with every workspace crate instrumented |

## What the tool found

Two engine attempts at wrap (`tddy-tools restructure apply`, cold rust-analyzer):

1. Five bottom-up `extract_method`s plus the helper move. Four extractions applied, and the fifth
   (`warmed_cli_harness_roster`, the roster warm-up) was **refused**: *rust-analyzer dropped 2
   comment(s) from the extracted function, and its statements cannot be paired with the range's to
   put them back*. The run was rolled back. Three of the four that applied also produced functions of
   12, 10 and 9 parameters (`spawn_cli_harness_jail`, `cli_harness_jail_env`,
   `provision_cli_harness_worktree`), and `clippy::too_many_arguments` under `-D warnings` rejects
   those.
2. Only the extraction that stays at seven parameters or fewer (the bridge and registration), plus
   the move. Applied, compile gate green.

## Why it matters here

This is the third copy of the sandboxed start sequence, beside `start_sandboxed_claude_cli_session`
(348 lines, its own record) and `start_sandboxed_cursor_cli_session` (430). Each copy drifts on its
own: the jail env, the warm-up gate and the metadata write are written three times.

## What would close it

Parameter structs first, written by hand as wiring (`JailSession` / `JailDirs` / `JailLaunch` in
`svc_start_sandboxed_claude_cli_session.rs` are the shape). Then the four extractions go through the
engine with no more than seven parameters each, and the roster warm-up range is cut so that its
leading comment stays in the caller. Better still, a shared launch sequence for all three sandboxed
starts, which waits on coverage
([`docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md`](../../../../docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md)).
This function is the first of the three with a passing jail acceptance suite on macOS
(`tddy-session-lifecycle/tests/sandboxed_cli_harness_acceptance.rs`), so it is the safe one to cut
first.

## Verified by hand

2026-10-22 — Read the function after the wrap's extraction. The length is the sequence above, with
no dead branch in it. Both engine runs are recorded in the measurement history.
