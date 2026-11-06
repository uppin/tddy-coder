# Changeset: Bounded session waits

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — four crates; cancellation/ordering semantics on the start path; process seams (supervisor socket, sshd)

## Initial Discovery

[2026-11-06-bounded-session-waits-initial-discovery.md](./2026-11-06-bounded-session-waits-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-acp-launch-waits-have-no-deadline.md](../todo/2026-11-04-acp-launch-waits-have-no-deadline.md) | ✅ RESOLVED HERE — the `open_session` half is already bounded (`SESSION_OPEN_DEADLINE`, `acp_driver.rs:227`); this node bounds the supervisor half and the clone stream the record missed |
| [2026-11-06-generic-session-start-has-no-deadline-of-its-own.md](../todo/2026-11-06-generic-session-start-has-no-deadline-of-its-own.md) | ✅ RESOLVED HERE |

The `clone_askpass` host-key hang has no record; this node reproduces it first.
No `Claimed by:` record in any touched package.

## Affected Packages

- **tddy-supervisor** — `client.rs` call and clone-stream deadlines.
- **tddy-daemon-kernel** — `session_start_timeout_secs` (default 600, 0 clamped to 1).
- **tddy-agent-launch** — `start_here` / `start_session_core` wrapped in the deadline; `clone_planner` if the host-key hang is real.
- **tddy-telegram-control** — `START_DEADLINE` removed in favour of the start deadline.

## Related Feature Documentation

- PRD: [PRD-2026-11-06-bounded-session-waits.md](../../ft/daemon/1-WIP/PRD-2026-11-06-bounded-session-waits.md)

## Summary

Every wait on the session-start path gets a deadline that fails with a named error: supervisor calls,
the supervisor clone stream, and the session start as a whole.

## Scope

- [ ] `SupervisorClient::call` bounded; `Unavailable` names method and deadline
- [ ] `clone_repository` bounded to its first frame and between frames
- [ ] `session_start_timeout_secs` (600 s default) cancels the start and cleans up; `DeadlineExceeded` names the running step
- [ ] Telegram uses the start deadline instead of its own
- [ ] `clone_askpass` host-key test run against the real sshd; fixed or recorded with evidence

## Technical Changes

### State A

- `tddy-supervisor/src/client.rs:291-292` `call()` awaits `transport.call_unary(…)` unbounded; `spawn_relayed` (:150) goes through it. `clone_repository` (:198-246) awaits `call_server_stream` and `frames.recv()` unbounded. `SupervisorError::Unavailable { message }` exists (`error.rs`). Crate style: module `const X: Duration` (`clone.rs:35`).
- `tddy-agent-launch/src/operator_start.rs:54` passes `CancellationToken::new()` into `start_here` (`svc_start_session_core.rs:144`); `start_session_core` likewise.
- Telegram `START_DEADLINE` 600 s (`tddy-telegram-control/src/telegram_session_control/cli_spawns.rs:12,93`).
- Knob pattern: `spawn_worker_request_timeout()` (`tddy-daemon-kernel/src/config/daemon_methods.rs:138`, clamp test `config.rs:541`).
- `clone_askpass_acceptance.rs:172` host-key test; guard `A_CLONE_THAT_HANGS` 60 s (`tests/support/rig.rs:25,101`); askpass branch `clone_planner.rs:345` (`SSH_ASKPASS_REQUIRE=force`, no `BatchMode`).

### State B

- `const SUPERVISOR_CALL_DEADLINE: Duration` and a clone first-frame / progress deadline; a `SupervisorClient` builder setter lets tests shorten them.
- `DaemonConfig::session_start_timeout()`; `start_here` runs its body under `tokio::time::timeout`, cancels the token on expiry, runs the existing cancelled-start cleanup, and returns `DeadlineExceeded` naming the step recorded by the start's progress.
- Telegram drops `START_DEADLINE` and relays the start's error.

### Delta

| Package | Change |
|---|---|
| tddy-supervisor | deadlines + test setter |
| tddy-daemon-kernel | config method + clamp test |
| tddy-agent-launch | deadline wrapper in `svc_start_session_core.rs` (thin — the `coding_agents` model check there is untouched) |
| tddy-telegram-control | delete `START_DEADLINE` |

## Testing Plan

The supervisor tests use a silent peer (a `UnixListener` that accepts and never answers), following
`a_git_that_never_finishes` (`supervisor_clone_progress.rs:184`). The start test injects a start step
that never returns. The host-key item runs the existing test against the real sshd fixture first;
its outcome decides whether a fix or a record follows.

## Acceptance Tests

| Test | File |
|---|---|
| `a_supervisor_that_never_answers_fails_spawn_relayed_naming_the_method` | `packages/tddy-supervisor/tests/supervisor_call_deadline.rs` |
| `a_supervisor_clone_with_no_first_frame_fails_within_its_deadline` | `packages/tddy-supervisor/tests/supervisor_clone_progress.rs` |
| `a_start_whose_step_never_returns_fails_deadline_exceeded_naming_the_step` | `packages/tddy-agent-launch/tests/session_start_deadline.rs` |
| `a_timed_out_start_leaves_no_partial_session` | `packages/tddy-agent-launch/tests/session_start_deadline.rs` |
| `a_zero_session_start_timeout_is_clamped` | `packages/tddy-daemon-kernel/src/config.rs` (`#[cfg(test)]`) |
| `a_host_key_confirmation_request_is_refused_and_the_clone_fails_in_seconds` (existing; must pass, not hang) | `packages/tddy-agent-launch/tests/clone_askpass_acceptance.rs` |

## Responsibility

Deadlines on the supervisor client and on the session start, and the host-key askpass clone.

## Boundaries

- Does **not** change the ACP `open_session` deadline or the ssh slot-write deadline.
- Does **not** touch `coding_agent_launcher.rs` (the session-pointer and acp-subagent-leftovers nodes do).
- Does **not** touch the `coding_agents` plumbing in `svc_start_session_core.rs` — `feature/agent-tools/host-registry`.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #828 | the start path and supervisor client as they are | bounds them | — |
| `feature/agent-tools/session-pointer`, `feature/agent-tools/host-registry` | branch-line parents only | — | touch pointer or registry code |

## Draft PR contract

The first push after this plan: `DaemonConfig::session_start_timeout()`, the `SupervisorClient`
deadline setter, plus the acceptance tests above failing (the host-key test's run result recorded in
this changeset).

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes
**Concurrent with:** `feature/agent-tools/session-pointer`, `feature/agent-tools/host-registry`, `feature/agent-tools/dead-launch-code`
**Blocks:** nothing

## Decisions & Trade-offs

- Default start deadline 600 s (developer, 2026-11-06) — the bound Telegram has today, so no start that succeeds now fails.
- The supervisor client gets a constant plus a test setter, not a config handle: the client has no config today and the daemon side already wraps some calls with `spawn_worker_request_timeout`.

## TODO

- [x] Record initial discovery
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
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Technical Debt & Production Readiness

_(populated during development)_

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
