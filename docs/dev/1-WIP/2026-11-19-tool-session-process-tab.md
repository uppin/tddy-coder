# Changeset: Tool sessions as a process tab

**Date**: 2026-11-19
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — several crates (spawn, supervisor client, cli-sessions, agent-launch, session-lifecycle), an exit signal across process boundaries (worker IPC), concurrency of exit vs stop
**Stack**: `#session-liveness` 3/4 (PR #960, stack #962) — branch `feature/session-liveness/tool-tab`, base `feature/session-liveness/tmux-roster`

## Affected Packages

- **tddy-supervisor**: `SignalSession` RPC — deliver a signal to a session it spawned (developer decision 2026-11-19)
- **tddy-task**: `ChildExit` and the exit channel (placed here, not in tddy-spawn, so the terminal registry does not depend on the supervisor)
- **tddy-spawn**: `SpawnedTool`, the exit report from each backend, and `WorkerEvent::Exited`
- **tddy-cli-sessions**: a process (PTY-less) tab in the terminal registry
- **tddy-agent-launch**: `spawn_tddy_coder` registers the tab; delete / signal go through it; the
  n1 carve-out is removed; `watch_until_stopped` waits on the tab
- **tddy-session-lifecycle**: acceptance tests

## Related Feature Documentation

- PRD: [PRD-2026-11-19-tool-session-process-tab.md](../../ft/daemon/1-WIP/PRD-2026-11-19-tool-session-process-tab.md)
- Discovery: [2026-11-19-tool-session-process-tab-initial-discovery.md](2026-11-19-tool-session-process-tab-initial-discovery.md)

## Summary

A tool session's tddy-coder becomes a process tab of its session, live until its spawn backend
reports the exit. Delete and signal reach it through that tab.

## Responsibility

- `tddy_spawn::ChildExit` and a daemon-side exit receiver returned with every tool spawn
  (`SpawnedTool { result: SpawnResult, exit: ExitReceiver }`), for each backend: the direct spawner's
  channel, `WorkerEvent::Exited`, and supervisor `SessionStatus` polling.
- `CliSessionManager::register_process_tab` — a PTY-less `main` tab of kind `tddy-coder`, counted by
  `live_agent_sessions`, not listed by `list_terminals`, removed on exit.
- Supervisor `SignalSession(SignalSessionRequest{session, signal}) → SessionStatus`, refused for a pid it never spawned, and `SupervisorClient::signal_session`.
- Stop and signal of a process tab through its holder; removal of the tool carve-out n1 left in
  delete and `signal_session_dir`.

## Boundaries

- Does not change how tddy-coder is spawned or what it writes to `.session.yaml`.
- No re-attach of a tool child after a daemon restart.
- No roster entry for tool sessions — the roster (2/4) records terminal tabs.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` agent-tab-liveness (#958) | the liveness rule, `live_agent_sessions`, delete/signal through held processes, and the `tool` carve-out | a process tab is one more held agent; this PR deletes the carve-out | change the rule for other kinds |
| `n2` tmux-roster | nothing consumed — n2 is below this node only because `gh stack` is a line | — | read or write `tabs.yaml` |

## Draft PR contract

The first push publishes `ChildExit`, `SpawnedTool`, `WorkerEvent::Exited`, and
`CliSessionManager::register_process_tab` as `todo!()`, plus the failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its tests read `is_active` and n1's delete/signal behaviour
**Concurrent with:** `tmux-roster` (2/4), once n1 is green
**Blocks:** none (4/4 sits above it only because of the line)

Real dependency edges: `n1 → n2, n3      n2 → n4`

## Prerequisites

### ✅ RESOLVED HERE — `tool` sessions list inactive and are still signalled by their recorded pid — [2026-11-19-tool-sessions-liveness-and-pid-signals.md](../todo/2026-11-19-tool-sessions-liveness-and-pid-signals.md)

Created by n1 as the interim; this node closes it.

### ⚠ DURING — The modules extracted into `tddy-spawn` are over the file budget — [2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md](../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md)

`spawner.rs` (2,152 lines) and `spawn_worker.rs` (568) must not grow. The exit plumbing goes in a new
`tddy-spawn/src/child_exit.rs`; the wait thread changes by one call.

### Code issues

`packages/tddy-agent-launch/docs/code-issues/complexity-tool-session-spawn-spawn-tool-session.md` —
⚠ during. The tab registration goes into `tool_session_spawned`, not into `spawn_tool_session`.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW — approved 2026-11-19
- [x] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy

## Technical Changes

### State A (Current)

`spawn_tddy_coder` returns `SpawnResult{pid,…}` only. The direct wait thread just logs. The worker
has no exit message. The supervisor is polled only during startup. Delete and signal use the
recorded pid (n1's carve-out). `watch_until_stopped` polls `process_exists(pid)`.

### State B (Target)

Each backend yields an `ExitReceiver` that resolves exactly once with `ChildExit`. The agent-launch
layer registers the process tab before returning to the caller and removes it on exit. The tab's
stop and signal go to its holder: supervisor `StopSession`, or a signal from the worker or direct
spawner to the child it owns.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/tool_session_liveness_acceptance.rs` (a stub tddy-coder that
waits for a release file):
- [~] `a_running_tool_session_lists_active`
- [~] `a_tool_session_whose_tddy_coder_exited_lists_inactive`
- [~] `deleting_a_tool_session_stops_its_tddy_coder_and_spares_its_recorded_pid`
- [~] `signalling_a_running_tool_session_reaches_its_tddy_coder`
- [~] `signalling_a_stopped_tool_session_is_refused`
- [~] `a_resumed_tool_session_is_live_again`

`packages/tddy-spawn/tests/child_exit_acceptance.rs`:
- [~] `the_forked_worker_reports_its_childs_exit_code`
- [~] `the_direct_spawner_reports_its_childs_exit_code`

## Contract as published

`ChildExit` and its channel live in **`tddy-task`**, not `tddy-spawn`: `tddy-cli-sessions` holds the
receiver, and depending on `tddy-spawn` would pull the spawn backends (`tddy-supervisor`, its `clap`
and `notify`) into the terminal registry. No cycle either way; `tddy-spawn` re-exports them.

- `packages/tddy-task/src/child_exit.rs` — `ChildExit` :18 (`with_code` :25, `by_signal` :33),
  `HolderGone` :44, `ExitSender` :56 (`send` :61), `ExitReceiver` :68 (`exited` :72),
  `exit_channel` :78. Implemented (a oneshot wrapper).
- `packages/tddy-spawn/src/child_exit.rs` — re-exports the above; `SpawnedTool` :30,
  `spawn_tool_as_user` :38, `SpawnClient::spawn_tool` :69, `spawn_tool_via_supervisor` :79,
  `watch_supervised_exit` :90, `exit_of(&SessionStatus)` :98 — all `todo!()`.
- `packages/tddy-spawn/src/spawn_worker/wire.rs:46` — `WorkerEvent::Exited { pid, code, signal }`
  (data only; `clone_backend.rs` ignores it for a clone).
- `packages/tddy-cli-sessions/src/cli_session_manager/process_tab.rs` — `TDDY_CODER_KIND` :18,
  `ProcessTabHolder` :23 (`stop`, `signal`), `ProcessTab` :34, `SignalRefused` :42,
  `CliSessionManager::register_process_tab` :52, `process_tab` :66, `signal_process_tab` :73 —
  `todo!()`.

## Unit Tests

- `tddy-spawn` `child_exit`: an `ExitReceiver` resolves once; the supervisor poller maps
  `EXITED{code}` to `ChildExit`.
- `tddy-cli-sessions` process tab: counted by `live_agent_sessions`, absent from `list_terminals`,
  gone after its exit resolves.

As written (`[~]` = failing, awaiting green):
- [x] `tddy-task` `child_exit::tests` — 2, pass (pure channel)
- [~] `tddy-spawn` `child_exit::tests` — 3 (`exit_of` mapping)
- [~] `tddy-cli-sessions` `process_tab_tests` — 7; `a_process_tab_makes_its_session_live` also
  needs n1's `live_agent_sessions`

## Decisions & Trade-offs

- **Signals to a supervised tool session go through a new supervisor `SignalSession` RPC** (developer decision 2026-11-19) — the supervisor owns the child, so it is the only party that can signal it exactly.
- **Supervisor exit by polling `SessionStatus`**: the supervisor owns the child, so the answer is exact.
  A streamed watch would need a supervisor proto change, which is not worth it for this node.
- **The process tab's id is `main`**, so code that asks "is main served" keeps working for tool
  sessions.
