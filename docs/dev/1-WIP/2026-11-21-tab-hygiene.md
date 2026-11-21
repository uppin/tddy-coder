# Changeset: Tab hygiene — a deleted tmux session leaves no agent, a signal never hits a reused pid

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 5 — several crates (cli-sessions, agent-launch, spawn, session-lifecycle, session-web, a new test-host crate), a new external dependency (sysinfo), a tab-roster property, reap/signal ordering concurrency in two backends plus a new worker request
**Stack**: `#tab-hardening` 2/4 — branch `feature/tab-hardening/tab-hygiene`, base `feature/tab-hardening/tab-errors`
**PR**: #990 https://github.com/uppin/tddy-coder/pull/990

## Initial Discovery

[2026-11-21-tab-hygiene-initial-discovery.md](./2026-11-21-tab-hygiene-initial-discovery.md)

## Affected Packages

- **tddy-cli-sessions**: `kill_tmux_session_logged`, `pane_pid_and_start`, process start time via `sysinfo` (new dependency, developer-approved)
- **tddy-changeset**: a property-key const only (`START_PROPERTY`, the tmux node's agent start) — no `SessionMetadata` field (developer decision 2026-11-21: the start lives in `tabs.yaml`)
- **tddy-agent-launch**: `tmux_main_roster` records the pane process's start on the tmux main's `tmux` node and reads it back; delete ends the pane process (`tmux_agent_end::end_tmux_run_agent`: TERM → grace → KILL, start-checked); `OwnedChild` signals through the backend's `ToolSignal` behind an injectable `deliver` seam; `claude_cli_tmux.rs` uses the shared kill helper
- **tddy-spawn**: `child_signal` — `ReapGate` (the reaper waits with `waitid(WNOWAIT)`, then marks ended and reaps under the lock every signal takes), `ToolSignal` on `SpawnedTool`, `Signalled`; `WorkerRequest::Signal` / `WorkerResponse::Signalled` so the forked worker signals its own child
- **tddy-session-lifecycle**: `recorded_tabs.rs` uses the shared kill helper; warmed stub scripts (`project_home_support`, `managed_codebase_workflow_acceptance`); fixtures from the new crate; the tmux-agent daemon fixture shared in `tests/tmux_agent_support`
- **tddy-test-hosts** (new crate): shared `ToolHost` test fixtures (no sandbox deps)
- **tddy-session-web**: `WrappedKindGroups` for the sandbox and target menu groups

## Related Feature Documentation

- PRD: [PRD-2026-11-21-tab-hygiene.md](../../ft/daemon/1-WIP/PRD-2026-11-21-tab-hygiene.md)

## Summary

Delete of a tmux-run session ends the agent (start-checked TERM → KILL); signals never reach a recycled pid in the direct and worker backends; two flaky suites fixed; repeated helpers, menu markup and test fixtures shared.

## Responsibility

- `tddy_cli_sessions::tmux::kill_tmux_session_logged(os_user, name, why)` replacing `kill_tmux_session_blocking` and `stop_tmux_session_of_failed_start`.
- `pane_pid_and_start(os_user, name) -> (u32, ProcessStart)` and `process_start(pid) -> Option<ProcessStart>` (sysinfo); the pane process's start recorded as the `start` property of the tmux main's `tmux` node in `tabs.yaml` when a tmux main starts (`record_tmux_main(…, start)`), read back by `recorded_tmux_main_start`.
- Delete: read pane pid + start → kill tmux → SIGTERM the pid if its start matches the roster's recorded start (or, for a roster without one, the start read before the kill) → SIGKILL after a grace if still matching.
- Direct backend: reap only after the ended flag is set under the lock `OwnedChild::signal/stop` hold; worker backend: `WorkerRequest::Signal { pid, signal }` answered by the worker under its reap lock.
- Flakes: `write_argv_and_env_script` (and its copy) through `write_executable`; confirm `tool_session_local_gh` (already warmed).
- New crate `tddy-test-hosts` with `a_tool_host()` builder replacing the ~20 `ToolHost` copies listed in the discovery; dedupe the remaining target fixtures of todo 2026-11-16.
- `WrappedKindGroups` React component; test ids unchanged.

## Boundaries

- No change to the supervisor backend (already exact) or to tab behaviour visible to users beyond delete.
- No size/length refactors (developer scope: functionality + DRY only).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `tab-errors` (1/4) | nothing consumed — the line puts it below | — | touch the tab error mapper or the probe |

No test of this node reaches a `TODO(tab-errors)` symbol: n1's red tests (`agent_probe`, `tab_launch_status`, `target_agent_tab_acceptance`) are in files this node does not touch, and every failure in this node's tests is a `TODO(tab-hygiene)`.

## Draft PR contract

The first push publishes the new helper signatures, `WorkerRequest::Signal`, the start-time field and the new crate skeleton with `todo!()` bodies, and the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes
**Concurrent with:** `tab-errors` (1/4), `jail-death-watch` (3/4)
**Blocks:** none

Real dependency edges: `n1 → n4` (the probe helper). n2 and n3 have none.

## Prerequisites

### ✅ RESOLVED HERE — [2026-11-21-tool-host-fixtures-repeat-across-the-tab-jail-tests.md](../todo/2026-11-21-tool-host-fixtures-repeat-across-the-tab-jail-tests.md)

Shared through the new `tddy-test-hosts` crate (developer decision: a new support crate, not a dependency on tddy-testing-commons).

### ✅ RESOLVED HERE — [2026-11-16-remote-shell-test-fixtures-are-duplicated.md](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md)

The remaining stand-in and stub fixtures move to shared support; copies that would close a dependency cycle (tddy-remote-shell tests, agent-launch src) stay and are named in the wrap.

### ⚠ DURING — [2026-11-20-session-liveness-oversized-files-after-the-stack.md](../todo/2026-11-20-session-liveness-oversized-files-after-the-stack.md)

Its new-tab-menu section is closed here; its file-size rows stay (out of this stack's scope).

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-test-util.md` — fixtures move out, never in.
- `packages/tddy-agent-launch/docs/code-issues/complexity-tool-session-spawn-spawn-tool-session.md` — not grown.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW (written and red; review pending)
- [x] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

Delete kills the tmux session only (`svc_signal_delete_session.rs:305`). No process start time is recorded or read; the tmux main's roster node carries `session` and `os_user` only. `OwnedChild` checks `has_ended()` then signals, while the reaper already freed the pid (`reap_in_background` → `child.wait()`); the worker reports `Exited` over IPC. Two near-identical tmux-kill wrappers; repeated menu groups; ~20 `ToolHost` copies; unwarmed stub scripts.

### State B (Target)

As Responsibility.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/delete_session.rs` (extended, private tmux, fixture `tests/tmux_agent_support`):
- [x] `deleting_a_tmux_run_session_whose_agent_ignores_sighup_leaves_no_agent`
- [x] `a_pane_pid_whose_start_time_changed_is_never_signalled`
- [x] `a_tmux_run_session_records_when_its_agent_started` (added: the roster's `start` is the pane process's)

`packages/tddy-spawn/tests/child_exit_acceptance.rs` (extended):
- [x] `a_signal_to_an_ended_direct_child_is_not_delivered`
- [x] `a_signal_to_an_ended_worker_child_is_not_delivered`
- [x] `a_signal_to_a_running_direct_child_reaches_it` (added: the positive half)
- [x] `a_signal_to_a_running_worker_child_reaches_it` (added: the positive half)

`packages/tddy-session-web/cypress/component/NewTerminalMenu{Sandbox,Target}Acceptance.cy.tsx` (unchanged, green 6/6 on the contract — `WrappedKindGroups` is not wired yet).

The existing `claude_cli_tmux_session_acceptance` delete tests (`deleting_a_tmux_run_session_kills_the_agent_and_the_tmux_session`, `…_ends_an_agent_that_ignores_hangup`, `deleting_a_session_whose_metadata_is_unreadable_still_kills_its_tmux_session`) go red on the contract too: the delete path now calls the `TODO(tab-hygiene)` helpers. They are the regression spec for green.

## Unit Tests

- [x] `process_start` (`tddy-cli-sessions/src/process_start.rs`): a live pid has a start; the same pid's start is stable; a fresh process's start is near now; a dead pid has none.
- [x] `pane_pid_and_start` / `kill_tmux_session_logged` (`tddy-cli-sessions/tests/tmux_pane_start_and_kill.rs`, private tmux): the pane's pid with its start; no pane for an absent session; the kill ends a session; "no session" returns quietly.
- [x] `ReapGate` (`tddy-spawn/src/child_signal.rs`): delivered before the end; refused after it; **the child is marked ended before its pid is reaped** (a signal sent during the reap waits for the lock and is refused); a failed delivery is the OS error; `wait_without_reaping` leaves the status reapable.
- [x] `OwnedChild` (`tddy-agent-launch/src/tool_process_tab.rs`, injectable `deliver` seam): a running child's signal goes through the backend (passes — existing behaviour through the seam); a signal the backend found ended is refused as ended; a stop the backend found ended sends no SIGKILL after the grace.
- [x] `recorded_tmux_main_start` (`tddy-agent-launch/src/tmux_main_roster.rs`): recorded start round-trips; read from the tmux node's `start`; none without one; none without a roster; a non-numeric start is an error.
- [x] `a_tool_host` (`tddy-test-hosts/src/lib.rs`): only the declared env; an empty value reads unset; PATH in order; real filesystem.
- [x] `WrappedKindGroups` (`tddy-session-web/cypress/component/WrappedKindGroups.cy.tsx`): group order; test ids by prefix and kind; the wrapped group is the menu's default list; a wrapped choice carries its wrapper, an unwrapped one none.

## Decisions & Trade-offs

- **The old `deleting_a_tmux_run_session_ends_an_agent_that_ignores_hangup` test is removed** (developer decision 2026-11-21): it passed only because its stub died on the closed pty; `deleting_a_tmux_run_session_whose_agent_ignores_sighup_leaves_no_agent` is the real check.

- **TERM then KILL after a grace, start-checked** (developer decision).
- **sysinfo for start times** (developer decision; new dependency). Resolution is whole seconds.
- **The start lives on the tmux main's roster node, not in `.session.yaml`** (developer decision 2026-11-21): a `SessionMetadata` field would have broken ~105 struct literals across 11 crates and an older daemon's `deny_unknown_fields` read; `tabs.yaml` keeps unknown properties verbatim.
- **Worker signals its own child** (developer decision): cross-platform, no pidfd.
- **A new crate for test hosts** (developer decision).

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
