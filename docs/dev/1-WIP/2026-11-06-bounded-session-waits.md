# Changeset: Bounded session waits

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — four crates; cancellation/ordering semantics on the start path; process seams (supervisor socket, sshd). Re-scored after the contract (2026-11-07): unchanged — supervisor half S/M, start half M (cancel → grace → undo ordering, a token-free undo, two entry points to wire)
**PR**: [#831](https://github.com/uppin/tddy-coder/pull/831) (`#agent-tools 23/28`), branch `feature/agent-tools/bounded-waits`, base `feature/agent-tools/host-registry`

## Initial Discovery

[2026-11-06-bounded-session-waits-initial-discovery.md](./2026-11-06-bounded-session-waits-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-acp-launch-waits-have-no-deadline.md](../todo/2026-11-04-acp-launch-waits-have-no-deadline.md) | ✅ RESOLVED HERE — the `open_session` half is already bounded (`SESSION_OPEN_DEADLINE`, `acp_driver.rs:227`); this node bounds the supervisor half and the clone stream the record missed |
| [2026-11-06-generic-session-start-has-no-deadline-of-its-own.md](../todo/2026-11-06-generic-session-start-has-no-deadline-of-its-own.md) | ✅ RESOLVED HERE |

The `clone_askpass` host-key hang has no record. It was run in the contract step and **did not reproduce** — see Decisions.
No `Claimed by:` record in any touched package.

## Affected Packages

- **tddy-supervisor** — `client.rs` call and clone-stream deadlines.
- **tddy-daemon-kernel** — `session_start_timeout_secs` (default 600, 0 clamped to 1).
- **tddy-agent-launch** — `start_deadline::StartDeadline`, and `start_here` / `start_session_for_os_user` run under it. `clone_planner` is **not** touched (the host-key hang did not reproduce).
- **tddy-telegram-control** — `START_DEADLINE` removed in favour of the start deadline.

## Related Feature Documentation

- PRD: [PRD-2026-11-06-bounded-session-waits.md](../../ft/daemon/1-WIP/PRD-2026-11-06-bounded-session-waits.md)

## Summary

Every wait on the session-start path gets a deadline that fails with a named error: supervisor calls,
the supervisor clone stream, and the session start as a whole.

## Scope

- [x] `SupervisorClient::call` bounded; `Unavailable` names method and deadline
- [x] `clone_repository` bounded to its first frame and between frames
- [x] `session_start_timeout_secs` (600 s default) cancels the start and cleans up; `DeadlineExceeded` names the running step — done for the session and branch; an operator start's peer-held parts (split codebase half, agent clones) are refused, not torn down (TODO(operator-start-undo), Technical Debt)
- [x] Telegram uses the start deadline instead of its own
- [x] `clone_askpass` host-key test run against the real sshd; **recorded with evidence, no fix** (Decisions)

## Technical Changes

### State A

- `tddy-supervisor/src/client.rs:291-292` `call()` awaits `transport.call_unary(…)` unbounded; `spawn_relayed` (:150) goes through it. `clone_repository` (:198-246) awaits `call_server_stream` and `frames.recv()` unbounded. `SupervisorError::Unavailable { message }` exists (`error.rs`). Crate style: module `const X: Duration` (`clone.rs:35`).
- `tddy-agent-launch/src/operator_start.rs:54` passes `CancellationToken::new()` into `start_here` (`svc_start_session_core.rs:144`); `start_session_core` likewise.
- Telegram `START_DEADLINE` 600 s (`tddy-telegram-control/src/telegram_session_control/cli_spawns.rs:12,93`).
- Knob pattern: `spawn_worker_request_timeout()` (`tddy-daemon-kernel/src/config/daemon_methods.rs:138`, clamp test `config.rs:541`).
- `clone_askpass_acceptance.rs:172` host-key test; guard `A_CLONE_THAT_HANGS` 60 s (`tests/support/rig.rs:25,101`); askpass branch `clone_planner.rs:345` (`SSH_ASKPASS_REQUIRE=force`, no `BatchMode`).

### State B

- `SUPERVISOR_CALL_DEADLINE` (60 s, every unary call) and `SUPERVISOR_CLONE_PROGRESS_DEADLINE` (300 s, the clone's first frame and each gap between frames); two builder setters, `with_call_deadline` and `with_clone_progress_deadline`, let tests shorten them. A miss fails `Unavailable` naming the method and the deadline.
- `DaemonConfig::session_start_timeout()` (`session_start_timeout_secs`, default 600, 0 clamped to 1 s).
- `tddy_agent_launch::start_deadline::StartDeadline` is the seam: `StartDeadline::new(deadline).run(progress, cancel, start, undo)`. It hands `start` a sink that forwards every event to `progress` and tracks the begun-and-not-ended phase (also for a discarding, unary sink). On expiry it cancels `cancel`, waits up to `START_CANCEL_GRACE` (30 s), runs `undo` on a session that came back, drops a start that did not, and returns `DEADLINE_EXCEEDED` naming the deadline and the running step. A start that answers the cancel with `CANCELLED` is still reported `DEADLINE_EXCEEDED`. Phase events reach `progress` **before** `run` returns (drain any forwarding first, as `start_task::run_start` does).
- `start_here` and `start_session_for_os_user` run under `StartDeadline::new(config.session_start_timeout())`; the undo is a **token-free** session delete, because the operator/Telegram start carries no session token (`start_task::undo_a_cancelled_start` deletes through a token).
- Telegram drops `START_DEADLINE` and relays the start's error.

### Delta

| Package | Change |
|---|---|
| tddy-supervisor | deadlines + test setter |
| tddy-daemon-kernel | config method + clamp test |
| tddy-agent-launch | new `start_deadline.rs` (`StartDeadline`, `START_CANCEL_GRACE`); thin wiring in `svc_start_session_core.rs` / `operator_start.rs` (the `coding_agents` model check is untouched); a token-free undo |
| tddy-telegram-control | delete `START_DEADLINE` |

## Testing Plan

The supervisor tests use a silent peer (a `UnixListener` that accepts and never answers), following
`a_git_that_never_finishes` (`supervisor_clone_progress.rs:184`). The start test injects a start step
that never returns. The host-key item runs the existing test against the real sshd fixture first;
its outcome decides whether a fix or a record follows.

## Acceptance Tests

Every wait in these tests has a deadline: the supervisor tests are bounded by a 10 s outer timeout, the start tests run on a paused clock under a 3600 s (virtual) outer timeout. Status at the contract commit (2026-11-07, macOS):

| Test | File | Status |
|---|---|---|
| `a_supervisor_that_never_answers_fails_spawn_relayed_naming_the_method` | `packages/tddy-supervisor/tests/supervisor_call_deadline.rs:144` | 🟢 passes |
| `every_unary_call_is_bounded_not_only_spawn_relayed` | `packages/tddy-supervisor/tests/supervisor_call_deadline.rs:163` | 🟢 passes |
| `a_supervisor_clone_with_no_first_frame_fails_within_its_deadline` | `packages/tddy-supervisor/tests/supervisor_call_deadline.rs:179` | 🟢 passes |
| `a_supervisor_clone_that_goes_quiet_after_starting_ends_its_stream_with_unavailable` | `packages/tddy-supervisor/tests/supervisor_call_deadline.rs:197` | 🟢 passes |
| `a_client_connects_with_the_production_deadlines_until_told_otherwise` | `packages/tddy-supervisor/tests/supervisor_call_deadline.rs:226` | guard, passes |
| `a_start_whose_step_never_returns_fails_deadline_exceeded_naming_the_step` | `packages/tddy-agent-launch/tests/session_start_deadline.rs:101` | 🟢 passes |
| `a_timed_out_start_leaves_no_partial_session` | `packages/tddy-agent-launch/tests/session_start_deadline.rs:130` | 🟢 passes |
| `a_zero_session_start_timeout_is_clamped` | `packages/tddy-daemon-kernel/src/config.rs:573` | 🟢 passes |
| `a_host_key_confirmation_request_is_refused_and_the_clone_fails_in_seconds` (existing) | `packages/tddy-agent-launch/tests/clone_askpass_acceptance.rs:172` | passes — 10/10 in the agent-launch run (2026-11-07) |

### Unit tests

| Test | File | Status |
|---|---|---|
| `the_running_step_is_named_when_the_caller_reports_nowhere` | `packages/tddy-agent-launch/tests/session_start_deadline.rs:175` | 🟢 passes |
| `a_start_that_stops_when_cancelled_by_its_deadline_fails_deadline_exceeded_not_cancelled` | `packages/tddy-agent-launch/tests/session_start_deadline.rs:198` | 🟢 passes |
| `a_start_inside_its_deadline_returns_its_own_result_uncancelled` | `packages/tddy-agent-launch/tests/session_start_deadline.rs:227` | guard, passes |
| `a_start_inside_its_deadline_fails_with_its_own_error` | `packages/tddy-agent-launch/tests/session_start_deadline.rs:249` | guard, passes |
| `the_starts_progress_still_reaches_the_caller` | `packages/tddy-agent-launch/tests/session_start_deadline.rs:267` | guard, passes — pins the progress-drain ordering |
| `the_session_start_timeout_defaults_to_ten_minutes` | `packages/tddy-daemon-kernel/src/config.rs:560` | guard, passes |
| `a_daemon_yaml_without_a_session_start_timeout_gets_the_default` | `packages/tddy-daemon-kernel/src/config.rs:567` | guard, passes |

### To be added in green

- ✅ A wiring test per entry point — `start_here` (via `start_session_core`) and `start_session_for_os_user` — with a short `session_start_timeout` and a stuck step, asserting `DEADLINE_EXCEEDED` naming the step and no session left behind. Green in `packages/tddy-session-lifecycle/tests/session_start_deadline_acceptance.rs`, not `tddy-agent-launch/tests/`: the harness that builds the `LaunchSessions` is `DaemonSessionHost`, which `tddy-agent-launch` cannot depend on without a cycle. Three tests: a unary `StartSession` and an operator start, each on a clone that never finishes (`DEADLINE_EXCEEDED` naming `clone`, no session directory); and an operator workspace start whose worktree cut answers inside the grace, which is undone (no session directory, no worktree, no branch).

### Baseline at the contract commit

`./test -p tddy-supervisor -p tddy-daemon-kernel -p tddy-telegram-control`: 671 passed / 0 failed before, 674 / 5 after (the 5 reds above). `./test -p tddy-agent-launch`: 241 / 4 before, 244 / 8 after; the 4 pre-existing failures are the session-pointer (n21, #829) contract reds in `cli_session_pointer_unit_tests`. The Linux-only supervisor suites (`supervisor_clone_progress.rs` etc.) do not run on macOS.

Green run (2026-11-07, macOS, scoped to the touched packages; full-workspace is CI's):
`./test -p tddy-supervisor` 254 passed / 0 failed; `./test -p tddy-daemon-kernel` 221 / 0; `./test -p tddy-agent-launch` 248 / 4, where the 4 are the n21 `cli_session_pointer_unit_tests` reds; `./test -p tddy-telegram-control` 204 / 0; `./test -p tddy-session-lifecycle --lib --test delete_session --test workspace_session_deletion_acceptance --test start_cancel_acceptance --test start_phase_acceptance --test session_start_deadline_acceptance --test workspace_session_start_acceptance` 185 + 2 + 3 + 5 + 3 + 2 + 13, all green.

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

Pushed in the contract commit, bodies marked `TODO(bounded-waits): implement`:

- `tddy_supervisor::client::{SUPERVISOR_CALL_DEADLINE, SUPERVISOR_CLONE_PROGRESS_DEADLINE}`, `SupervisorClient::with_call_deadline(Duration)`, `SupervisorClient::with_clone_progress_deadline(Duration)`.
- `DaemonConfig::session_start_timeout_secs` (serde default 600) and `DaemonConfig::session_start_timeout()`.
- `tddy_agent_launch::start_deadline::{StartDeadline, START_CANCEL_GRACE}`: `StartDeadline::new`, `with_cancel_grace`, `run(progress, cancel, start, undo)`.
- The failing tests above; the host-key test's run result is recorded under Decisions.

**Deviations from the plan:** both supervisor tests live in the new `supervisor_call_deadline.rs` rather than in `supervisor_clone_progress.rs`, which is Linux-only, so they run on every platform (the peer is a socket, not a forked supervisor). It also gained a between-frames test not in the original table. There are two setters rather than one. The start deadline is the generic `StartDeadline` seam (after the `start_keepalive.rs` precedent) rather than a `tokio::time::timeout` written inline in `start_here`.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes
**Concurrent with:** `feature/agent-tools/session-pointer`, `feature/agent-tools/host-registry`, `feature/agent-tools/dead-launch-code`
**Blocks:** nothing

## Decisions & Trade-offs

- Default start deadline 600 s (developer, 2026-11-06) — the bound Telegram has today, so no start that succeeds now fails.
- The supervisor client gets constants plus test setters, not a config handle: the client has no config today and the daemon side already wraps some calls with `spawn_worker_request_timeout`. Values: 60 s per unary call, 300 s for the clone's first frame and between frames (developer, 2026-11-07).
- `START_CANCEL_GRACE` 30 s: on expiry cancel → wait the grace → undo a session that came back → drop a start that never returns (developer, 2026-11-07). A dropped start can still leave a partially cut worktree; `TODO(start-cancel)` in `start_task.rs` already tracks the in-flight worktree cut.
- The undo must be token-free: the operator/Telegram start carries no session token (developer, 2026-11-07).
- **`clone_askpass` host-key hang: not reproduced, no fix** (developer, 2026-11-07). `a_host_key_confirmation_request_is_refused_and_the_clone_fails_in_seconds` passed 3/3 in isolation in 2.4–3.1 s, and in two full-suite runs (10/10), on macOS with nix OpenSSH_10.2p1 against the real sshd fixture. `tddy-askpass/src/main.rs:21` refuses any prompt that is not a key passphrase request, so ssh fails "Host key verification failed" at once. Linux/CI and older OpenSSH were not checked. `clone_planner.rs` is untouched.
- **Green: the start's events are read by the deadline's own task** (`start_deadline.rs`, `Relay`), not by a spawned forwarder. A forwarder lags the timer, so the step named at expiry could be the wrong one. Reading the relay's channel in the same task is exact, and needs no bounded drain, since nothing is awaited to close. This departs from the brief's "spawn a forwarder".
- **Green: the deadline is `sleep_until` raced against the start, not `tokio::time::timeout`.** A timeout wrapper drops the future at expiry, and the grace must keep polling it.
- **Green: the wrap is at `start_here`,** which both entry points reach: `start_session_core_cancellable` → `start_session_with_role` → `start_session_on_request` → `start_here`, and `start_session_for_os_user` → `start_here`. The body is now `start_here_body`.
- **Green, open: an operator start's peer-held parts are refused, not torn down.** The undo is keyed by OS user (`undo_a_timed_out_start`, `delete_session_as`, `branch_a_start_cut_as`), so the session and its branch are removed without a token. The split codebase half and the agent clones are still deleted through `SplitSessions::delete_paired_codebase_session` and `AgentRoster::tear_down_every_agent_clone` with the start's own token, which an operator start does not have. For an operator start with either, the undo is refused and the session is left in place, and that is logged. Unblocking it needs an OS-user-keyed delete on the split host port (`tddy-session-split`) and the peer delete protocol. TODO(operator-start-undo) at `start_task.rs`. **Decided (user, 2026-11-07): planned as a follow-up stack node.** The fail-closed refusal and the log stay in this node.
- **Green: an undo failure is logged, not returned.** `StartDeadline::run`'s `undo` returns `()`, which the red tests fix, so the caller's `DEADLINE_EXCEEDED` does not name a session left behind.
- **Green: a clone's opening and its first event share one deadline** (`timeout_at` on one instant). Each later event gets the full deadline.
- **Green, open, not fixed: a dropped in-flight call leaves its pending entry.** `StdioRpcClient::call_unary` registers its reply in `ClientEngine::pending_unary` (`packages/tddy-rpc/src/client_engine.rs`), and dropping the future does not remove it. The entry goes when a reply arrives for its id or the connection closes; `call_server_stream` leaves a `pending_streams` entry the same way. A late reply is dropped on arrival, but the supervisor may already have acted on a timed-out request (a spawn, a scope), and the daemon is not told. Reported; no workaround in `tddy-rpc`.
- **Green: `clippy -D warnings` is clean in the nix toolchain** (`./dev cargo clippy -p tddy-supervisor -p tddy-daemon-kernel -p tddy-agent-launch -p tddy-telegram-control -p tddy-session-lifecycle --all-targets -- -D warnings`, 2026-11-07, exit 0, no diagnostics). An earlier run under the non-nix cargo toolchain was red in untouched crates (`double_must_use` at `packages/tddy-task/src/task.rs:421`, `useless_conversion` in `tddy-build`). That toolchain is not the gate, so the red did not reproduce under nix.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Green: wiring test per entry point (`start_here`, `start_session_for_os_user`) — short timeout, stuck step
- [x] Green: token-free undo of a timed-out start's session (operator/Telegram path has no token) — the session and its branch; the peer-held parts are refused, TODO(operator-start-undo) (Technical Debt)
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

- **Operator-start undo of peer-held parts** (`TODO(operator-start-undo)`, `packages/tddy-agent-launch/src/start_task.rs`). An operator start's split codebase half and agent clones are deleted with the start's own session token, which an operator start does not have, so a timed-out operator start that created either is refused and left in place, and that is logged. The session and its branch are removed. Planned as a follow-up stack node (user, 2026-11-07): an OS-user-keyed delete on the split host port (`tddy-session-split`, `svc_paired_codebase_teardown.rs:78`, `:103`) and in the agent roster (`tddy-session-agents`, `svc_provision_agent_clone.rs:253`), plus the peer delete protocol.
- **Pending-entry leak in tddy-rpc** (`packages/tddy-rpc/src/client_engine.rs:70`, `:105-123`). A dropped in-flight supervisor call leaves its pending entry until a reply arrives or the connection closes. Recorded in `docs/dev/todo/2026-11-07-rpc-dropped-call-leaves-pending-entry.md`; not fixed here, since the engine is shared by every client.
- **Duplicated teardown.** `remove_what_a_start_created` (`start_task.rs`) repeats the teardown in `undo_a_cancelled_start`. Two undo paths now have to stay in sync. Not unified here, so the cancelled-start path keeps its behaviour and its tests unchanged.
- **Network-dependent wiring stuck step.** `packages/tddy-session-lifecycle/tests/session_start_deadline_acceptance.rs` makes its stuck step an SSH clone to 192.0.2.1 (TEST-NET-1), the `start_cancel_acceptance` technique. It relies on that address hanging rather than being refused. Where it is refused at once, the test fails loudly rather than passing falsely. A deterministic stuck step would need production changes, so none was added. The grace-then-drop path is covered by unit tests only.

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
