# heavy-params: relaunch_sandboxed_runner

**Location:** `packages/tddy-agent-launch/src/svc_relaunch_sandboxed_runner.rs:67` — `LaunchSessions::relaunch_sandboxed_runner`
**Category:** heavy-params
**Detected:** 2026-11-02 — `/pr-wrap` step 4 (`/analyze-clean-code`) on PR #794 (`#agent-tools` 5/12)
**Metrics:** **14 parameters** + `self` (12 at the base) · 129 lines (168 at the base) · budget 5 parameters, 60 lines
**Thresholds breached:** parameters 14 > 5 (**2.8×**); length 129 > 60
**Restructure:** an options struct — a hand change
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02) — **unclaimed**

## Measurement history

| Run | Metric | Note |
|---|---|---|
| 2026-11-02 | 14 params, 129 lines | first detection; 12 params and 168 lines at the base `eae01cf5c` |

## What the tool found

PR #794 added `project_id` and `session_token`, which the shared jail launcher's `JailSession` names. Five of the parameters (`session_id`, `project_id`, `session_token`, `session_dir`, `worktree_path`) are exactly `JailSession`'s fields, which the body then assembles. The body got shorter (168 → 129) because the hand-built runner launch moved behind `launch_jailed_claude`.

## What would close it

Take `jail: JailSession<'_>` in place of the five fields (14 → 10), then a relaunch request struct for the rest; the one caller is `svc_resume_sandboxed_claude_cli_session.rs`.
