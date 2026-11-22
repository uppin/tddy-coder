# complexity: LaunchSessions::resume_cli_harness

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn/resume.rs` — `LaunchSessions::resume_cli_harness`
**Moved:** 2026-11-07 by #829 (`#agent-tools` 21/28), from `opencode_cli_spawn.rs` to `opencode_cli_spawn/resume.rs`
**Category:** complexity
**Detected:** 2026-11-06 — `/pr-wrap` on #823 (`#agent-tools` 17/19), fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run
**Metrics:** **88 lines**
**Thresholds breached:** length 88 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04)
**Status:** Open — partially fixed by #924 (`#remote-shell` 8/11): restored to below its before-the-stack length (83); still over 60 — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-06 | 65 → 88 | first detection — #823: a split session's wiring is re-derived first (`resume_split_wiring`), and its context directory and empty commit identity chosen, now that every claude-cli session resumes here. Not split: restructuring deferred to the sweep node (n20) |
| 2026-11-07 | 87 → 87 | **unchanged, moved** in #829 (`#agent-tools` 21/28): engine `move_item` of the `LaunchSessions` impl into `opencode_cli_spawn/resume.rs`; fn line to closing brace on `origin/master` (`d3fe1abea`) and HEAD |
| 2026-11-16 | 95 → 108 | #921 (`#remote-shell` 7/7): the resume path carries the session's subject and Coder credentials to the target resolution. Touched, not closed |
| 2026-11-16 | 108 → 83 | #924 (`#remote-shell` 8/11) sweep: restored to below its before-the-stack length (88). Still over 60, so this record stays open on the remainder |
| 2026-11-20 | 86 → 83 | #961 (`#session-liveness` 4/4): the resume restores the roster after main (`restore_roster_on_resume`, +6) and the target shell is resolved by `resumed_target_shell` (−9); fn line to closing brace, this script, on `e430092a8` (86) and HEAD. Better by 3, still over 60 |
| 2026-11-21 | 84 → 84 | #980 (`#orchestra` 2/9): touched (its call into the wiring), unchanged; brace-matched fn line to closing brace on `origin/feature/orchestra/path-reach` (`c4dace05c`) and HEAD |
| 2026-11-22 | 84 → 87 | #998 (resume main-launch claim): **worse by 3** — the claim taken first and the `resuming` log line (both helpers in `resume_guard.rs`). Brace-matched fn line to closing brace on `origin/master` (`0a76c98fb`) and HEAD. Still over 60 |
| 2026-11-22 | 87 → 87 | #992 (`#tab-hardening` 4/4): the resume's one agent statement became `agent_to_launch(kind, &target_shell)` (a target resume probes its agent first); kept on one line, so the function is unchanged — fn line to closing brace by `awk` on `origin/master` (after #998) and HEAD, brace-matched. Touched, not closed |

## What would close it

Move the split half (`resume_split_wiring`, the worktree choice, the identity choice) into one `fn resumed_placement(...) -> ResumedPlacement` beside it (`/code-restructuring` `extract_method`).
