# 2026-11-21 — Tab hygiene: a deleted tmux session leaves no agent, a signal never hits a reused pid

**Type:** Bug Fix · **Stack:** `#tab-hardening` 2/4 (PR #990) · **Packages:** `tddy-cli-sessions`, `tddy-changeset`,
`tddy-agent-launch`, `tddy-spawn`, `tddy-session-lifecycle`, `tddy-session-web`, `tddy-test-hosts` (new) ·
Feature: [session-liveness.md](../../ft/daemon/session-liveness.md#deleting-a-tmux-run-session)

## Problem

Deleting a tmux-run session killed its tmux session only. An agent that ignores `SIGHUP` and holds no terminal
survived, running with its session directory gone. In the worker and direct spawn backends a signal could go out
between the kernel reaping a tool child and the daemon marking it ended, reaching whatever process had reused the
pid. Alongside, two suites flaked on macOS's slow first exec of a fresh script, and the same helper, menu markup and
`ToolHost` fixture existed in several copies.

## What changed

- **Delete ends the agent** (`tddy-agent-launch::tmux_agent_end::end_tmux_run_agent`): read the pane's pid and its
  start (`tddy_cli_sessions::tmux::pane_pid_and_start`, `process_start::process_start` over `sysinfo`), kill the tmux
  session, `SIGTERM` the pid and `SIGKILL` it after a 5 s grace — each only while the pid's start still equals the
  start recorded on the tmux main's `tmux` roster node (`tab_roster::START_PROPERTY`; read back by
  `recorded_tmux_main_start`), or the start read before the kill for a roster that kept none.
- **A signal never reaches a recycled pid** (`tddy-spawn::child_signal`): the reaper waits with `waitid(WNOWAIT)`,
  then marks the child ended and reaps it under a `ReapGate` lock that every signal takes. `SpawnedTool.signal` is a
  `ToolSignal` — `Direct` under the daemon's gate, `Worker` through the new `WorkerRequest::Signal` /
  `WorkerResponse::Signalled`, answered by the worker under the gate of the child it holds. `OwnedChild` signals through
  it; a signal found ended is refused as ended and a stop of one sends no `SIGKILL`. A spawn's gate and exit report
  travel together as `ChildEnd`, so `spawn_as_user_reporting` gained no parameter.
- **Flakes**: `write_argv_and_env_script` (and its copy in `managed_codebase_workflow_acceptance`) write through the
  warmed `write_executable`; `tool_session_local_gh` was already warmed.
- **One copy each**: `kill_tmux_session_logged` replaces `kill_tmux_session_blocking` and
  `stop_tmux_session_of_failed_start` (and the open-coded kill in the restore path); `WrappedKindGroups` /
  `MenuKindItem` replace the sandbox and target menu blocks of `NewTerminalMenu` (473 → 424 production lines, test ids
  unchanged); the new `tddy-test-hosts` crate's `a_tool_host()` replaces `AHostOfStubs`, `AHostOfBinaries`,
  `AStubPathHost`, `AHostWithoutAnyAgent` and every `AFixtureHost` / `AHostOf`; `AnAgentRun` and the tmux-agent daemon
  fixture moved to shared `tests/` support.

## Decisions & Trade-offs

- **TERM, then KILL after a grace, start-checked** (developer decision). Start times have whole-second resolution, so a
  pid reissued to a process that started in the same second is not told apart.
- **`sysinfo` for start times** (developer decision, a new dependency): the same on macOS and Linux.
- **The start lives on the tmux main's roster node, not in `.session.yaml`** (developer decision): a `SessionMetadata`
  field would have broken ~105 struct literals across 11 crates and an older daemon's `deny_unknown_fields` read;
  `tabs.yaml` keeps unknown properties verbatim.
- **The worker signals its own child** (developer decision): cross-platform, no `pidfd`.
- **A crate of its own for test hosts** (developer decision): `tddy-session-lifecycle::test_util` is a public module, so
  `tddy-testing-commons` (sandbox crates) cannot be its dependency.
- **The old `deleting_a_tmux_run_session_ends_an_agent_that_ignores_hangup` test is gone** (developer decision): it
  passed because its stub died on the closed pty; the new `…_whose_agent_ignores_sighup_leaves_no_agent` stub holds no
  terminal and only a signal to its pid ends it.
- **The supervisor backend is unchanged**: it signals the pid it spawned and was already exact.

## Backlog

- `docs/dev/todo/2026-11-21-tool-host-fixtures-repeat-across-the-tab-jail-tests.md` and
  `docs/dev/todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md` are **narrowed, not closed**: what remains
  would close a dependency cycle (`tddy-actions`' own tests; `tddy-remote-shell`, `tddy-agent-launch/src`) or has
  drifted between callers.
- `docs/dev/todo/2026-11-20-session-liveness-oversized-files-after-the-stack.md` loses its new-tab-menu section.
- `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-test-util.md`: 578 → 556, still over, not split
  (`#tab-hardening` 3/4 and 4/4 also edit `test_util.rs`).

## Tests

`tddy-session-lifecycle/tests/delete_session.rs` (private tmux; `tests/tmux_agent_support`):
`deleting_a_tmux_run_session_whose_agent_ignores_sighup_leaves_no_agent`, `a_pane_pid_whose_start_time_changed_is_never_signalled`,
`a_tmux_run_session_records_when_its_agent_started`. `tddy-spawn/tests/child_exit_acceptance.rs`: a signal to an ended /
a running child, direct and worker. Unit: `process_start`, `pane_pid_and_start` / `kill_tmux_session_logged`
(`tmux_pane_start_and_kill.rs`), `ReapGate` (including that the child is marked ended before its pid is reaped),
`OwnedChild` through its `deliver` seam, `recorded_tmux_main_start`, `a_tool_host`, and the Cypress component spec
`WrappedKindGroups.cy.tsx`.
