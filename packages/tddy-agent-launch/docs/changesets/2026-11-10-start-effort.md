# 2026-11-10 — Start a session at an effort level

**Type:** Feature

- `refuse_an_effort_the_agent_cannot_take` (`start_request_checks.rs`), called from the start checks beside `refuse_a_cli_start_without_a_model`: a non-empty effort its agent's spec does not declare, or any for an agent with no `effort` slot, is `INVALID_ARGUMENT` naming the agent and its levels.
- `effort_launch` (`effort_launch.rs`) turns a level into the spec's carry (`EffortLaunch { args, env }`). The host start (`opencode_cli_spawn.rs`) and resume (`opencode_cli_spawn/resume.rs`) add it after the role's flags and ahead of the route's; the jailed start and resume carry it through `CliJail.effort` (`cli_harness_jail.rs`). No agent is named in Rust.
- The level is recorded in `.session.yaml` on the host path and in the jail record (`JailRecord.effort`), and resumes re-apply it. `HarnessLaunch` gained no field: the carry rides `slots.argv_extra` and the launch env.

Cross-package: [effort on session start](../../../../docs/dev/changesets/2026-11-10-start-effort.md). Product: [Claude CLI session § Effort](../../../../docs/ft/daemon/claude-cli-session.md#effort).
