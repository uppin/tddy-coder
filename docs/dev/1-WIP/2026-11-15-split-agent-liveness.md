# Changeset: A split session's agent either comes up or the start fails with its own words

**Date**: 2026-11-15
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — three crates (`tddy-pty`, `tddy-cli-sessions`, `tddy-session-split`), a spawned-process seam, a cross-host acceptance test over the LiveKit testkit harness
**Stack**: `#split-alive` 1/4 — branch `feature/split-alive/agent-liveness`, base `master`
**PR**: [#905](https://github.com/uppin/tddy-coder/pull/905)

## Initial Discovery

[2026-11-15-split-agent-liveness-initial-discovery.md](./2026-11-15-split-agent-liveness-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-15-split-session-agent-not-running.md](../todo/2026-11-15-split-session-agent-not-running.md) | ✅ **RESOLVED HERE** (lowest node of the split fix; the entry was written on `investigate/split-session-agent-not-running` and is carried in by this node's plan commit). This node closes fixes 1 and 2 and makes item A's open cause self-reporting; `feature/split-alive/split-hooks` (fix 3), `feature/split-alive/split-targets` (fix 5) and `feature/split-alive/split-pairing` (fixes 6–7) close the rest and keep the reference. Fix 4 (the exit cause itself) is re-recorded as a new TODO at wrap if it is still unknown |
| [packages/tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md](../../../packages/tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md) (233 lines, unclaimed) | ⚠ **During** — the startup watch and the recording registration live in their own module (`svc_spawn_split_agent/startup.rs`); `spawn_split_agent` gains at most the call |
| [packages/tddy-pty/docs/code-issues/oversized-file-runtime.md](../../../packages/tddy-pty/docs/code-issues/oversized-file-runtime.md) (523 > 500), [complexity-runtime-spawn-child-and-pump.md](../../../packages/tddy-pty/docs/code-issues/complexity-runtime-spawn-child-and-pump.md) (67 lines) | ⚠ **During** — the exit-status mapping is a new function in a new module (`tddy-pty/src/exit_status.rs`); the monitor's two-arm `match` becomes one call, so neither record grows |

## Affected Packages

- **tddy-pty**: [README.md](../../../packages/tddy-pty/README.md) — the exit monitor reports the child's real exit code, and a signal as a failure naming it
- **tddy-cli-sessions**: [README.md](../../../packages/tddy-cli-sessions/README.md) — one log line per ended `main` terminal
- **tddy-session-split**: [README.md](../../../packages/tddy-session-split/README.md) — the split spawn records its terminal and watches its startup
- **tddy-daemon** (tests only): `tests/remote_managed_worktree_cross_host_acceptance.rs`

## Related Feature Documentation

- [PRD-2026-11-15-split-agent-liveness.md](../../ft/daemon/1-WIP/PRD-2026-11-15-split-agent-liveness.md)
- [remote-managed-worktree.md § Session start](../../ft/daemon/remote-managed-worktree.md#session-start)

## Summary

The split agent's `main` terminal is recorded, every ended `main` is logged with its real outcome
and lifetime, the PTY runtime stops reporting every exit as code 0, and a split start fails —
quoting the agent's last output and tearing down the codebase half — when its agent exits inside the
daemon's startup window.

## Background

See the PRD and [the findings](../todo/2026-11-15-split-session-agent-not-running.md) § A. Four split
agents died unseen; the start said `succeeded`; nothing recorded what they printed.

## Responsibility

- `tddy_pty::exit_status::task_status_of(&portable_pty::ExitStatus) -> TaskStatus` and its use in the
  exit monitor.
- The ended-`main` log line in `CliSessionManager::spawn_terminal_cleanup`, through a pure formatter
  `cli_session_manager::main_exit::describe_main_exit(session_id, pid, &TaskStatus, Duration) -> String`.
- The split spawn's terminal recording (`record_main_capture` at
  `main_capture_recording_path(tddy_data_dir, session_id)`) and its startup watch
  (`svc_spawn_split_agent/startup.rs`: `SplitAgentStartup`, `await_split_agent_startup`), wired into
  `spawn_split_agent` after the process is spawned and before metadata is written.
- The startup-failure error text: `split agent exited during startup (<outcome>): <last lines>`.

## Boundaries

- Does **not** change why the agent exits — this node reports it.
- Does **not** touch split hooks or `--setting-sources` (`feature/split-alive/split-hooks`), the
  inspection tools (`feature/split-alive/split-targets`), the start ledger's branch/worktree or the
  drawer (`feature/split-alive/split-pairing`).
- Does **not** change `TaskStatus` (`tddy-task`); a signal is reported as `Failed { message }`.
- Does **not** add a split-specific window: the daemon's `spawn_startup_grace_period_ms` /
  `spawn_startup_poll_interval_ms` are reused (PRD decision, 2026-11-15).
- Does **not** change co-located or jailed launch behaviour beyond the shared exit log and exit code.

## Dependencies

Root node: no parent in this stack. It consumes only `master`.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| — (`master`) | `record_main_capture`, `main_capture_recording_path` (`tddy-cli-sessions`), `DaemonConfig::spawn_startup_grace_period` / `spawn_startup_poll_interval` (`tddy-daemon-kernel`), the agent-step teardown (`split_claude_cli_start.rs:187-200`) | calls them | change their signatures |

## Draft PR contract

The first push of this PR (pass 2, commit 2) publishes:

- `packages/tddy-pty/src/exit_status.rs` — `pub fn task_status_of(status: &portable_pty::ExitStatus) -> TaskStatus` (`// TODO(agent-liveness): implement`).
- `packages/tddy-cli-sessions/src/cli_session_manager/main_exit.rs` — `pub(crate) fn describe_main_exit(...) -> String` (stub).
- `packages/tddy-session-split/src/svc_spawn_split_agent/startup.rs` — `SplitAgentStartup { grace, poll }` with `from_config`,
  `StartupOutcome { Survived, Exited }`, `await_split_agent_startup(done: watch::Receiver<bool>, SplitAgentStartup) -> StartupOutcome`
  (watches `PtyHandle::pty_done`), `SplitAgentDied { outcome, last_output }` with `from_capture(outcome, capture_bytes)` and
  `Display` (stubs). How the outcome string is obtained for an ended terminal (the task's `TaskStatus`, which `PtyHandle` does
  not expose today) is the implementer's call inside `tddy-cli-sessions` / `tddy-session-split`.
- The monitor already calls `task_status_of` (stub keeps today's behaviour).
- The failing tests listed below.

**Published (commit 2):** as above; `describe_main_exit(session_id, pid, &TaskStatus, Duration)` in
`tddy-cli-sessions/src/cli_session_manager/main_exit.rs`; `task_status_of` in `tddy-pty/src/exit_status.rs`.

## Green wave

**Wave:** 1 of 1
**Greenable independently:** yes — every test drives this node's own surface; the acceptance tests use the existing cross-host harness and a stub agent
**Concurrent with:** `feature/split-alive/split-hooks`, `feature/split-alive/split-targets`, `feature/split-alive/split-pairing`
**Blocks:** none (no other node consumes this node's surface)

Real dependency edges, as opposed to the branch line: none.

## Scope

- [x] PTY exit monitor reports the real exit code; a signal is a failure naming it
- [x] Ended `main` terminals are logged with session, pid, outcome and lifetime
- [x] The split agent's terminal is recorded in its session dir
- [x] A split start fails, with the agent's last output, when its agent exits inside the startup window; the codebase half is torn down
- [x] Docs: `remote-managed-worktree.md` § Session start states the startup window and the recording

## Technical Changes

### State A

- `tddy-pty/src/runtime.rs:353-354`: `Ok(_) => TaskStatus::Completed { exit_code: Some(0) }` for every exit.
- `tddy-cli-sessions/.../pty_spawn.rs:241-295`: the cleanup task removes an ended terminal silently.
- `tddy-session-split/src/svc_spawn_split_agent.rs:144-185`: spawn → metadata → `Ok`; no recording
  (`claude_cli_spawn.rs:7-17` and `jail_launch_steps.rs:25` are the only `record_main_capture` callers).
- `split_claude_cli_start.rs:187-200`: an `Err` from `spawn_split_agent` tears down the codebase half.

### State B

- `task_status_of`: `exit_code()` on a normal exit → `Completed { exit_code: Some(code) }`; `signal()`
  → `Failed { message: "terminated by signal <SIG>" }`.
- `spawn_terminal_cleanup` logs, for `MAIN_TERMINAL_ID`, `describe_main_exit(..)` at `info` (non-zero
  or signal at `warn`), target `tddy_daemon::claude_cli_session`, e.g.
  `main exited: session=<id> pid=<pid> outcome=exit code 3 ran=1.2s`.
- `spawn_split_agent`: after `spawn_split_agent_process`, `record_main_capture(session_id,
  main_capture_recording_path(tddy_data_dir, session_id))`, then
  `await_split_agent_startup(cli_manager, session_id, &handle, SplitAgentStartup::from_config(config))`.
  On `Err(died)` it returns `Status::failed_precondition(died.to_string())` (last ≤ 20 non-empty lines of
  the capture, ANSI stripped); the caller's existing `Err` arm tears down the codebase half. The agent
  process is already gone, so nothing is stopped; its pointer is removed by its own exit hook.

### Delta

| Package | Change |
|---|---|
| `tddy-pty` | new `exit_status.rs`; monitor calls it |
| `tddy-cli-sessions` | new `main_exit.rs`; one log call in `spawn_terminal_cleanup` |
| `tddy-session-split` | new `svc_spawn_split_agent/startup.rs`; two calls in `spawn_split_agent` |
| `tddy-daemon` | two acceptance tests |
| docs | `docs/ft/daemon/remote-managed-worktree.md` § Session start |

## Implementation Milestones

- [x] `task_status_of` + monitor wiring, unit tests green
- [x] `describe_main_exit` + log call, unit tests green
- [x] split recording registered; recording acceptance green
- [x] startup watch; failure acceptance green; existing cross-host suite still green

## Testing Plan

**Levels.** Unit tests for the two pure functions; a PTY-level test for real exit codes (real child
process, no daemon); acceptance tests on the existing two-daemon harness, because the teardown under
test spans both hosts.

**Trade-off.** The cross-host harness runs over the LiveKit testkit, which the repo keeps for
compatibility rather than behaviour; it is the only harness that runs a split start end to end, and
the teardown it proves is the point of the feature. The startup watch itself is unit-tested with a
fake terminal status so its timing is not tested through LiveKit.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_split_agent_that_exits_during_startup_fails_the_start_with_its_last_output` | `packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs` | stub agent prints `boom: cannot reach the codebase` and exits 3: `start_session` errs; message contains `exit code 3` and the printed line; codebase host sessions unchanged |
| `a_split_agents_terminal_is_recorded_in_its_session_dir` | same | stub prints a marker then reads stdin: `<agent sessions base>/sessions/<id>/terminals/main.capture` eventually holds the marker |
| `a_child_that_exits_non_zero_reports_its_exit_code` | `packages/tddy-pty/src/runtime.rs` (tests) | `sh -c 'exit 3'` → `Completed { exit_code: Some(3) }` |
| `a_child_killed_by_a_signal_reports_the_signal` | same | `sh -c 'kill -9 $$'` → `Failed` naming `SIGKILL`/`Killed` |

## Unit / Integration Tests

| Test | File |
|---|---|
| `a_normal_exit_keeps_its_code`, `a_clean_exit_is_a_completion_with_code_zero` (passes today, guards the mapping), `a_signal_is_a_failure_naming_it` | `packages/tddy-pty/src/exit_status.rs` |
| `an_ended_main_is_described_with_its_outcome_and_lifetime`, `a_signalled_main_names_the_signal`, `a_cancelled_main_says_the_daemon_stopped_it` | `packages/tddy-cli-sessions/src/cli_session_manager/main_exit.rs` |
| `an_agent_still_running_after_the_window_has_started` (passes on the stub), `an_agent_that_exits_inside_the_window_has_not_started`, `an_agent_whose_terminal_is_already_gone_has_not_started`, `the_last_lines_are_capped_and_stripped_of_terminal_escapes`, `a_death_reads_as_a_startup_failure_quoting_the_agent` | `packages/tddy-session-split/src/svc_spawn_split_agent/startup.rs` |
| `a_split_agent_that_exits_during_startup_fails_the_start_with_its_last_output`, `a_split_agents_terminal_is_recorded_in_its_session_dir` | `packages/tddy-daemon/tests/remote_managed_worktree_cross_host_acceptance.rs` (acceptance, above) |

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer for this stack, 2026-11-15: "finish both passes without approvals")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green
- [ ] Validate changes
- [ ] Wrap docs

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Reuse the daemon's startup window** (500 ms default) rather than a split-specific one — user
  decision on the PRD, 2026-11-15. Deaths after the window are reported by the exit log and kept in
  the recording, not by failing the start.
- **Signal → `Failed`**, not a new `TaskStatus` field: keeps `tddy-task` untouched.

## Refactoring Needed

### From /validate-changes

### From /pr-wrap

## Validation Results

### /validate-changes

### /pr-wrap
