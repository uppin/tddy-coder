# 2026-11-22 — A resume refuses a duplicate or running launch of a session's agent

**Type:** Fix · **PR:** #998 · **Packages:** `tddy-cli-sessions`, `tddy-agent-launch`,
`tddy-session-lifecycle` (tests) ·
Feature: [claude-cli-session.md § Resume after process exit](../../ft/daemon/claude-cli-session.md#resume-after-process-exit)

## Problem

A stopped claude-cli session on a Coder target was reported as resuming to no effect. The daemon log
showed otherwise: each `ResumeSession` spawned `claude --resume` on the workspace about 2 minutes
later, with nothing logged in between, and the operator's second click launched a **second** agent on
the same remote worktree. Its registration replaced the first under `main` in `CliSessionManager`'s
terminal index, so Stop reached only one of them; the other ran on orphaned. Nothing refused a resume
of a session whose agent was running or already being launched — locally the window is well under a
second, over a target it is minutes.

## What changed

- **`tddy-cli-sessions`**: `CliSessionManager::claim_main_launch(session_id)`
  (`cli_session_manager/main_launch_claim.rs`) — an RAII `MainLaunchClaim`, inserted before the first
  await, refused as `MainLaunchRefusal::Launching` while another claim is held and as `Running` when
  the session's `main` is registered.
- **`tddy-agent-launch`**: `resume_cli_harness` takes the claim as its first step and holds it until it
  returns (`opencode_cli_spawn/resume_guard.rs`); a refusal is `FAILED_PRECONDITION`
  — `session <id> is already being resumed: wait for that resume to finish` /
  `session <id> is already running: stop it before resuming it`. The resume logs `resuming <kind>
  session <id> (target host: …)` and `resumed … pid <n>`.

## Tests

`tddy-session-lifecycle/tests/coder_target_session_acceptance.rs`:
`a_second_resume_while_the_first_is_still_launching_is_refused_and_one_agent_runs`,
`a_resume_of_a_session_whose_agent_is_still_running_is_refused`.
`host_without_integrations.rs` now waits for the started stub agent to end before resuming — it had
resumed a session whose exited agent's terminal was still registered.

## Deferred

- [A sandboxed resume takes no main-launch claim](../todo/2026-11-22-sandboxed-resume-has-no-launch-claim.md)
- [A Coder-target resume takes ~2 minutes and shows no progress](../todo/2026-11-22-coder-target-resume-shows-no-progress.md)
- [Stopping a Coder-target session reads as `exit code 1`](../todo/2026-11-22-coder-target-stop-reads-exit-code-1.md)
