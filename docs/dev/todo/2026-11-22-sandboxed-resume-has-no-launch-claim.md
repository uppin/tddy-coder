# 2026-11-22 — A sandboxed resume takes no main-launch claim

**Category:** Deferred from `fix/coder-target-resume` (#998)
**Source:** `packages/tddy-agent-launch/src/svc_resume_sandboxed_cli_harness_session.rs` (`resume_sandboxed_cli_harness`); the guard it lacks is `CliSessionManager::claim_main_launch` (`packages/tddy-cli-sessions/src/cli_session_manager/main_launch_claim.rs`)

#998 made a host-PTY resume (`resume_cli_harness`, local or on a target) claim its session's main
launch, so a second resume while one is in flight, or a resume of a session whose agent is up, is
refused instead of launching a second agent. A **sandboxed** session resumes through
`resume_sandboxed_cli_harness`, which relaunches its jail without that claim: a double Resume there
can still race two relaunches of the same jailed agent.

**Why deferred:** the reported defect was on a Coder target, and a sandboxed agent's liveness is held
by `SandboxSessionManager`, not by `CliSessionManager`'s `main` terminal alone, so the "already
running" half needs its own check there. Out of #998's scope.

**What would close it:** take the same claim at the top of `resume_sandboxed_cli_harness`, refuse a
running jail by what the sandbox manager says is live, and add a double-resume acceptance test beside
`sandboxed_session_lifecycle_acceptance.rs`.
