# Changeset: Project-home session auto-stop on idle

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — concurrency / timing semantics (periodic task vs turns), daemon config, across tddy-daemon-kernel, tddy-agent-launch, tddy-daemon; integration tests with a stub harness
**Stack**: #project-home 6/7. Base: `feature/project-home/home-session-tools`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-22-project-home-auto-stop-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Responsibility

- `ProjectHomeConfig { idle_stop_secs }` in the daemon config (default 600, 0 = off).
- The pure idle policy: given a home session's agent status, last turn end and last input, and now — stop or not.
- The periodic task that applies it to running project-home sessions and stops them through the existing stop path.

## Boundaries

- Does not change ensure, resume or the conversation capture — it relies on them (node 4 and the existing resume paths).
- Never stops a session whose role is not `project-home`.
- No UI; the drawer already shows a stopped dot.
- Does not touch the tools of nodes 5 and 7.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/project-home/home-session` (4/7) | `session_role`, `EnsureProjectHomeSession` (resume path) | selects sessions by role; the resume-after-stop test calls ensure | change ensure or resume |
| `feature/project-home/home-session-tools` (5/7) | session tools | nothing — position only (the line puts 5 before 6 because 5 blocks 7) | — |

## Draft PR contract

```rust
// tddy-daemon-kernel::config::sections
#[derive(Deserialize)] pub struct ProjectHomeConfig { #[serde(default = "default_project_home_idle_stop_secs")] pub idle_stop_secs: u64 }
// tddy-agent-launch::project_home_idle (new)
pub struct IdleObservation { pub agent_status: SessionAgentStatus, pub last_turn_end: Option<SystemTime>, pub last_input: Option<SystemTime> }
pub fn should_stop(obs: &IdleObservation, now: SystemTime, idle_stop: Duration) -> bool;
pub async fn reap_idle_project_home_sessions(ctx: &ProjectHomeIdleContext) -> Vec<String>; // stopped ids
```
Plus the failing tests below. Bodies `// TODO(home-auto-stop): implement`.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — needs node 4's working ensure/role to have a home session to stop and to resume.
**Concurrent with:** `feature/project-home/home-session-tools` (5/7)
**Blocks:** none

Real dependency edges, as opposed to the branch line:

    n1 home-agent-setting → n4      n2 project-groups → n4      n3 activity-sidebar → n4      n4 home-session → n5, n6      n5 home-session-tools → n7

Waves: 1: n1, n2, n3 · 2: n4 · 3: n5, n6 · 4: n7 — line `1 1 1 2 3 3 4`.

## Affected Packages

- **tddy-daemon-kernel**: [README.md](../../../packages/tddy-daemon-kernel/README.md) — config section
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) — idle policy + reaper
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — spawn the periodic task (`src/runtime/tasks.rs`)

## Related Feature Documentation

- PRD: [PRD-2026-10-22-project-home-auto-stop.md](../../ft/daemon/1-WIP/PRD-2026-10-22-project-home-auto-stop.md)

## Summary

Stops idle project-home sessions after a configurable time, keeping their conversation for resume.

## Background

No per-session idle stop exists; `tddy_task::IdleTimeoutTracker` serves the relay and LSP registry only.

## Scope

- [ ] Config section
- [ ] Idle policy
- [ ] Reaper task + runtime wiring
- [ ] Tests

## Technical Changes

### State A
No session is ever stopped for idleness. Agent status (`idle | running | executing_tool |
waiting_for_input | connecting | error | unknown`) and `last_activity` are on `SessionEntry`. Stop and
resume paths exist and keep the agent conversation.

### State B
A periodic task stops idle project-home sessions per policy; ensure resumes them.

### Delta
- **tddy-daemon-kernel**: `ProjectHomeConfig` + default.
- **tddy-agent-launch**: `project_home_idle.rs` (new).
- **tddy-daemon**: spawn the task in `runtime/tasks.rs`.

## Testing Plan

Pure-function table for `should_stop`; daemon acceptance with `idle_stop_secs` set to 1 and the stub
harness reporting turn end, polling `ListSessions` until stopped (bounded wait, no sleeps-as-asserts).

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `an_idle_project_home_session_is_stopped_after_the_configured_time` | `packages/tddy-daemon-rpc/tests/project_home_idle_stop_acceptance.rs` (new) | stop |
| `a_project_home_session_mid_turn_is_not_stopped` | same | running guard |
| `ensuring_an_auto_stopped_home_session_resumes_the_same_conversation` | same | conversation kept |
| `an_ordinary_session_is_never_auto_stopped` | same | role scope |
| `idle_stop_secs_zero_disables_auto_stop` | same | off switch |
| `stops_only_when_idle_since_the_later_of_turn_end_and_input` | `packages/tddy-agent-launch/src/project_home_idle.rs` (`#[cfg(test)]`) | policy table |

## Decisions & Trade-offs
- **Idle = after turn, no input** (developer's choice, 2026-10-22), not "on leaving the view": the daemon cannot see the view, and a background turn should finish.
- **Periodic task**, not a per-session timer: survives daemon restarts by re-reading state; granularity is the period (≤ 30 s).

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-22-project-home-auto-stop-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-project-home-auto-stop-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
