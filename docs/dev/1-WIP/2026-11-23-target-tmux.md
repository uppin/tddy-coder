# Changeset: the agent of a target session runs in tmux on the target (`#target-tmux` 1/2)

**Date**: 2026-11-23
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate (tddy-agent-launch, tddy-session-driver), a spawned-process seam (remote shell + real tmux in acceptance tests), start/resume/end semantics

PRD: [PRD-2026-11-23-target-tmux.md](../../ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md) ·
Discovery: [2026-11-23-target-tmux-initial-discovery.md](2026-11-23-target-tmux-initial-discovery.md)

## Responsibility

`run_in_tmux` on a target start/resume: tmux preflight on the target, the agent run inside
`tmux new-session -A -s tddy-<session>` on the target, re-attach on resume, kill on end.

## Boundaries

- **In:** `remote_agent_launch` (command, start judgement, owner), `target_start_agent` (preflight),
  harness start/resume wiring, `svc_signal_delete_session` (target branch).
  `tddy-session-driver` only gains `tmux: false` (it never asks for a tmux run).
- **Out:** tmux tabs on targets — node 2. Automatic reconnect — TODO.

## Dependencies

None (base `master`).

## Decisions & Trade-offs (developer, 2026-11-23)

| Fork | Decision |
|---|---|
| PID tracking | **The tmux session name is the identity.** No pid file is written for a tmux run — the agent is the tmux server's child, not tddy's. Startup liveness = `tmux has-session`; the pointer owner pid is read from tmux (`#{pane_pid}`). Stop/delete kill by name. |
| Dropped connection / resume | The session goes inactive; the agent keeps running. Resume runs `new-session -A`: re-attach if alive, fresh start (resume argv) if gone. No auto-reconnect. |
| tmux tabs on targets | In scope — node 2 of a small stack. |
| PR shape | Small stack: n1 (this), n2 (tabs). |

Every tmux command on the target runs through the target's **login shell** (as the agent probe and
the agent launch do), so a tmux found only on the login `PATH` is the one probed and the one run.

## State A

See discovery. `run_in_tmux` with `target` is ignored; the agent runs under the transport client and
dies with it; delete would kill a **local** tmux session if `tmux_session` were recorded.

## State B — draft PR contract

`packages/tddy-agent-launch/src/remote_agent_launch.rs`:

- `RemoteLaunch::tmux: bool` — run the agent in tmux on the target (no 7th parameter on
  `launch_on_target`; see `complexity-remote-agent-launch-launch-on-target.md` below).
- `pub fn target_tmux_session_name(session_id) -> String` = `agent_tmux_session_name`.
- `pub fn remote_agent_tmux_command(worktree, session_id, argv, env) -> String`:
  `cd '<worktree>' && exec "${SHELL:-/bin/sh}" -lc 'exec tmux new-session -A -s '\''tddy-<id>'\'' '\''<target_login_exec>'\'''`
- `pub async fn end_target_tmux_session(shell, session_id) -> Result<(), RemoteLaunchError>` —
  `tmux kill-session -t '=tddy-<id>'` through the login shell; a missing session/server is `Ok`.
- `RemoteLaunchError::TmuxNotOnTarget`.
- `pub async fn probe_tmux_on_target(shell, worktree) -> Result<(), RemoteLaunchError>`.

`packages/tddy-agent-launch/src/target_start_agent.rs`:
`require_tmux_on_the_target(shell) -> Result<(), Status>` (FAILED_PRECONDITION naming tmux,
UNAVAILABLE for an unreachable target), called by start and resume when `run_in_tmux` and a target.

Wiring (green): `harness_launch::launch` sets `RemoteLaunch::tmux` from `HarnessRun`; the launched
harness reports `tmux_session` so metadata records it; delete calls `end_target_tmux_session` for a
target session instead of the local kill; `tddy-session-driver` stop likewise.

## Acceptance tests

`packages/tddy-agent-launch/tests/remote_agent_launch_acceptance.rs` (real tmux on a private
`TMUX_TMPDIR` inside the target's filesystem):

- `an_agent_run_in_tmux_runs_in_a_tmux_session_named_for_the_session_on_the_target`
- `an_agent_run_in_tmux_outlives_its_dropped_connection`
- `a_second_launch_in_tmux_re_attaches_to_the_running_agent`
- `ending_a_tmux_run_kills_its_tmux_session_on_the_target`
- `ending_a_tmux_run_whose_session_is_gone_is_not_an_error`

Daemon (`packages/tddy-session-lifecycle/tests/remote_target_session_acceptance.rs`, the shared
`StandInSshTarget` now gives its machine a private `TMUX_TMPDIR`):

- `a_tmux_run_on_the_target_runs_the_agent_in_tmux_there_and_records_its_tmux_session`
- `a_tmux_run_whose_connection_drops_keeps_its_agent_and_resumes_attached_to_it`
- `a_tmux_run_whose_agent_ended_resumes_its_agent_in_tmux_on_the_target`
- `deleting_a_tmux_run_kills_its_tmux_session_and_agent_on_the_target`

## Implementation notes

- `RemoteAgentPid` (`cli_session_pointer.rs`): the start wait and the owner record read the agent's
  pid from the pid file or, for a tmux run, from tmux (`display-message -p -t '=<name>:'
  '#{pane_pid}'` — `=<name>` alone names no pane and prints an empty line).
- Host-tmux paths skip target sessions: delete (`target_tmux_run_of`), resume's unwatched-exit settle,
  and the daemon-start re-attach (`pre_roster_main`).
- Stop detaches (local client only), matching host tmux runs.

## Unit tests

- `remote_agent_launch.rs`: `the_tmux_command_runs_the_login_exec_in_a_tmux_session_named_for_the_session`
- `remote_agent_launch/agent_probe_tests.rs`: `the_tmux_probe_asks_the_targets_login_shell_for_tmux`,
  `a_target_whose_login_shell_finds_no_tmux_has_no_tmux`
- `target_start_agent_tests.rs`: `a_tmux_run_on_a_target_without_tmux_is_refused_naming_tmux`

## Prerequisites (Step 2b)

- ⚠ During — `packages/tddy-agent-launch/docs/code-issues/complexity-remote-agent-launch-launch-on-target.md`:
  6 parameters > 5. The tmux flag goes on `RemoteLaunch`, never a new parameter.
- — [2026-11-22-coder-target-stop-reads-exit-code-1.md](../todo/2026-11-22-coder-target-stop-reads-exit-code-1.md):
  unrelated (stop classification); unchanged by a tmux run.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Acceptance tests (failing — `ending_a_tmux_run_whose_session_is_gone_is_not_an_error` passes against the stub by construction)
- [x] Unit tests (failing)
- [x] Implementation
- [x] TDD green — `./test -p tddy-agent-launch` green; `remote_target_session_acceptance` 17/17 (4 new daemon tests: start, resume re-attach, resume after the agent ended, delete); clippy clean on agent-launch, session-lifecycle, testing-commons, session-driver
- [x] TODO entry: [2026-11-23-target-tmux-auto-reconnect.md](../todo/2026-11-23-target-tmux-auto-reconnect.md)
