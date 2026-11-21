# Changeset: A dead jail ends its session's terminal

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — two crates (daemon-sandbox, session-lifecycle) plus agent-launch call sites, exit-watch concurrency against stop/reap (re-scored at the contract: unchanged — 9 failing tests, one new module, three test fixtures folded)
**Stack**: `#tab-hardening` 3/4 — branch `feature/tab-hardening/jail-death-watch`, base `feature/tab-hardening/tab-hygiene`
**PR**: #991 https://github.com/uppin/tddy-coder/pull/991

## Initial Discovery

[2026-11-21-jail-death-watch-initial-discovery.md](./2026-11-21-jail-death-watch-initial-discovery.md)

## Affected Packages

- **tddy-daemon-sandbox**: a main-jail exit watch (new module), the recorded `ChildExit`, a `JailProcess` handle abstraction, kept relay/endpoint join handles, `SandboxSessionManager::subscribe_ended`
- **tddy-session-lifecycle**: `SandboxTerminalSession` pty_done fires on the jail's exit; the session is recorded inactive
- **tddy-agent-launch**: `jail_launch.rs` / `tab_jail_launch.rs` keep the bridge handles

## Related Feature Documentation

- PRD: [PRD-2026-11-21-jail-death-watch.md](../../ft/daemon/1-WIP/PRD-2026-11-21-jail-death-watch.md)

## Summary

A main jail that dies on its own ends its terminal (status recorded), leaves the registry and makes the session inactive and resumable; tab jails are untouched; a stopped jail is not announced.

## Responsibility

- `watch_jail_exit` started when `SandboxSessionManager::insert` registers a main: waits on the runner without reaping (`waitid(WNOWAIT)` on a blocking task, no lock held across the wait), records `ChildExit`, fires pty_done, removes the jail (if still the registered one), broadcasts `JailEnded { session_id, exit }`.
- `JailProcess` enum (`Owned(Child)` now) behind `SandboxSessionState` so a supervised jail can plug in later.
- `dial_and_bridge_with_egress` returns a `JailBridge` of its join handles; the state keeps and aborts them on stop.
- `SandboxTerminalSession::subscribe_pty_done` driven by the exit, not the stdout broadcast closing.
- On a jail's death the session's `.session.yaml` is set inactive (the existing stop path).

## Boundaries

- No uid change for jails — jail-uid is split out (developer decision) and recorded as a todo by this node.
- Tab jails keep their own death watch; no change to stop/delete/resume semantics.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `tab-hygiene` (2/4) | nothing consumed — the line puts it below | — | touch the spawn backends or tmux |

## Draft PR contract

The first push publishes `JailProcess`, `JailBridge`, `JailEnded`, `subscribe_ended` and `watch_jail_exit` as `todo!()`, and the failing tests below.

Published surface (all in the new module `packages/tddy-daemon-sandbox/src/jail_exit_watch.rs`, bodies `todo!()` unless noted):

- `pub struct JailEnded { session_id: String, exit: tddy_task::ChildExit }`
- `pub enum JailProcess { Owned(std::process::Child) }` + `From<SandboxHandle>` (implemented); `pid`, `try_exit` (`WNOHANG | WNOWAIT`), `kill_and_reap`
- `pub fn wait_without_reaping(pid: u32) -> io::Result<ChildExit>` — blocking `waitid(WEXITED | WNOWAIT)`
- `pub async fn watch_jail_exit(state: Arc<SandboxSessionState>) -> Option<ChildExit>` — `Some` for a death, `None` for a requested stop; records the exit on the state
- `pub struct JailBridge` + `new(endpoint, relay, forwarder)` (implemented) + `abort`
- `SandboxSessionState::{with_bridge, exit, subscribe_exit}`; `SandboxSessionManager::subscribe_ended`
- `dial_and_bridge` / `dial_and_bridge_with_egress` now return `Result<JailBridge, String>` (implemented: the three handles they used to drop). The two agent-launch call sites bind it as `_bridge` with a `TODO(jail-death-watch)` to hand it to `with_bridge` — dropping it detaches the tasks, exactly as before.

Wiring left for green, each marked `TODO(jail-death-watch)`: `SandboxSessionManager::insert` starts the watch; `SandboxTerminalSession::subscribe_pty_done` follows `subscribe_exit`; `is_running`/`runner_pid`/`stop` go through `JailProcess` so a liveness poll never reaps the status away.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — tests build a `SandboxSessionState` over a stand-in process (`a_sandbox_running`)
**Concurrent with:** `tab-errors` (1/4), `tab-hygiene` (2/4)
**Blocks:** none (jail-uid, planned later, will build on `JailProcess`)

Real dependency edges: `n1 → n4` (the probe helper). n2 and n3 have none.

## Prerequisites

### ✅ RESOLVED HERE — [2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)

Its open half (nothing watches a main runner for death) is closed; delete the entry at wrap.

### ➕ NEW TODO — jail-uid

Recorded by this node at wrap (developer decision 2026-11-21): jails run as the daemon's uid; running them as the session's OS user goes through the root supervisor (`SpawnSandbox` lacks profile/egress shim; stdio via relay socket; cross-uid files and persistent home; Linux only), building on `JailProcess`.

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-daemon-sandbox/docs/code-issues/oversized-file-sandbox-session.md` — the watch is a new module.
- `packages/tddy-daemon-sandbox/docs/code-issues/heavy-params-sandbox-session-dial-and-bridge.md` — no new parameter; returning `JailBridge` changes only the return type.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [x] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

No exit event or status for a main jail; `is_running()` `try_wait` reaps and loses the status; bridge join handles dropped; pty_done fires only when the state is dropped.

### State B (Target)

As Responsibility.

## Acceptance Tests

`packages/tddy-daemon-sandbox/src/sandbox_session_liveness_tests.rs` (extended):
- [x] `a_main_jail_that_exits_is_announced_ended_with_its_status`
- [x] `a_main_jail_stopped_by_request_is_not_announced`
- [x] `a_main_jail_that_exits_leaves_the_registry`

`packages/tddy-session-lifecycle/tests/sandbox_terminal_parity_acceptance.rs` (extended):
- [x] `a_sandboxed_sessions_terminal_ends_when_its_jail_dies`
- [x] `a_sandboxed_session_whose_jail_died_lists_inactive_and_keeps_its_tab_jails`

Pre-existing tests this node changed, because their premise is the behaviour it replaces:
- `a_sandboxed_session_whose_runner_exited_is_not_live` no longer asserts that the dead jail's entry stays registered (it now leaves — the third test above).
- The parity suite's jail stand-in was an already-exited `true`; it is now a running `sleep` (`a_stand_in_runner`), because a registered dead jail now ends its terminal and leaves the registry.

## Unit Tests

- `watch_jail_exit`: status recorded for `exit 3` and for a signal; a stop during the wait is silent.
- `JailBridge`: handles aborted on stop.

Written in `packages/tddy-daemon-sandbox/src/jail_exit_watch_tests.rs`: `waiting_without_reaping_leaves_the_exit_for_its_owner_to_reap`, `a_jail_process_reads_its_exit_without_reaping_it`, `the_watch_records_the_status_of_a_runner_that_exits_3`, `the_watch_records_the_signal_that_killed_its_runner`, `the_watch_keeps_the_status_when_liveness_was_polled_first`, `the_watch_is_silent_for_a_runner_stopped_during_the_wait`, `aborting_a_bridge_ends_every_task_of_it`, `a_jails_stop_aborts_the_bridge_it_holds`.

Test fixture: the stand-in jail built three times in this crate (`a_sandbox_running`, `a_main_jail_running`, `a_sleeping_jail`) is one, `src/jail_test_support.rs`; in tddy-session-lifecycle the parity suite, `sandbox_tab_support` and `fake_tab_jails` share `a_stand_in_runner`.

Running them: the session-lifecycle tab suites need `tddy-tools` prebuilt (`cargo build -p tddy-tools-cli`, `TDDY_TOOLS_BINARY=$PWD/target/debug/tddy-tools`), and the daemon-sandbox lib needs `tddy-sandbox-runner` prebuilt.

## Decisions & Trade-offs

- **End like a terminal, resumable; tab jails untouched** (developer decision).
- **Wait without reaping** so the status is never lost to `is_running()`.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
