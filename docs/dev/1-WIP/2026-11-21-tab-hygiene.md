# Changeset: Tab hygiene — a deleted tmux session leaves no agent, a signal never hits a reused pid

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 5 — several crates (cli-sessions, agent-launch, spawn, session-lifecycle, session-web, testing-commons, a new test-host crate), a new external dependency (sysinfo), a session-metadata field, reap/signal ordering concurrency in two backends
**Stack**: `#tab-hardening` 2/4 — branch `feature/tab-hardening/tab-hygiene`, base `feature/tab-hardening/tab-errors`

## Initial Discovery

[2026-11-21-tab-hygiene-initial-discovery.md](./2026-11-21-tab-hygiene-initial-discovery.md)

## Affected Packages

- **tddy-cli-sessions**: `kill_tmux_session_logged`, `pane_pid_and_start`, process start time via `sysinfo` (new dependency, developer-approved)
- **tddy-changeset**: the agent's recorded start time beside its pid in `.session.yaml`
- **tddy-agent-launch**: delete ends the pane process (TERM → grace → KILL, start-checked); `OwnedChild` signals under the reap lock; `claude_cli_tmux.rs` uses the shared kill helper
- **tddy-spawn**: direct reaper waits with `waitid(WNOWAIT)` and marks ended under the signal lock; `WorkerRequest::Signal` so the forked worker signals its own child
- **tddy-session-lifecycle**: `recorded_tabs.rs` uses the shared kill helper; warmed stub scripts (`project_home_support`, `managed_codebase_workflow_acceptance`); fixtures from the new crate
- **tddy-test-hosts** (new crate): shared `ToolHost` test fixtures (no sandbox deps)
- **tddy-session-web**: `WrappedKindGroups` for the sandbox and target menu groups

## Related Feature Documentation

- PRD: [PRD-2026-11-21-tab-hygiene.md](../../ft/daemon/1-WIP/PRD-2026-11-21-tab-hygiene.md)

## Summary

Delete of a tmux-run session ends the agent (start-checked TERM → KILL); signals never reach a recycled pid in the direct and worker backends; two flaky suites fixed; repeated helpers, menu markup and test fixtures shared.

## Responsibility

- `tddy_cli_sessions::tmux::kill_tmux_session_logged(os_user, name, why)` replacing `kill_tmux_session_blocking` and `stop_tmux_session_of_failed_start`.
- `pane_pid_and_start(os_user, name) -> (u32, ProcessStart)` and `process_start(pid) -> Option<ProcessStart>` (sysinfo); recorded start stored beside the pid when a tmux main starts.
- Delete: read pane pid + start → kill tmux → SIGTERM the pid if its start matches → SIGKILL after a grace if still matching.
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
- [ ] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [ ] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

Delete kills the tmux session only (`svc_signal_delete_session.rs:305`). No process start time is recorded or read. `OwnedChild` checks `has_ended()` then signals, while the reaper already freed the pid (`reap_in_background` → `child.wait()`); the worker reports `Exited` over IPC. Two near-identical tmux-kill wrappers; repeated menu groups; ~20 `ToolHost` copies; unwarmed stub scripts.

### State B (Target)

As Responsibility.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/delete_session.rs` (extended, private tmux):
- [ ] `deleting_a_tmux_run_session_whose_agent_ignores_sighup_leaves_no_agent`
- [ ] `a_pane_pid_whose_start_time_changed_is_never_signalled`

`packages/tddy-spawn/tests/child_exit_acceptance.rs` (extended):
- [ ] `a_signal_to_an_ended_direct_child_is_not_delivered`
- [ ] `a_signal_to_an_ended_worker_child_is_not_delivered`

`packages/tddy-session-web/cypress/component/NewTerminalMenu{Sandbox,Target}Acceptance.cy.tsx` (unchanged, must stay green).

## Unit Tests

- `process_start`: a live pid has a start; the same pid's start is stable; a dead pid has none.
- `kill_tmux_session_logged`: "no session" is not an error.
- Direct reaper: ended is set before the pid is reaped (injectable `deliver` seam on `OwnedChild`).
- `WrappedKindGroups`: renders both groups with the given test-id prefixes.

## Decisions & Trade-offs

- **TERM then KILL after a grace, start-checked** (developer decision).
- **sysinfo for start times** (developer decision; new dependency).
- **Worker signals its own child** (developer decision): cross-platform, no pidfd.
- **A new crate for test hosts** (developer decision).

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
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
