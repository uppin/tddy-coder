# Changeset: Session liveness from live agent tabs

**Date**: 2026-11-19
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — many crates (kernel, session-activity, cli-sessions, daemon-sandbox, agent-launch, session-lifecycle, daemon-rpc, worktree-service, telegram-control), a cross-crate API change (`list_sessions_in_dir`), concurrency of PTY exit vs listing
**Stack**: `#session-liveness` 1/4 (PR #958, stack #962) — branch `feature/session-liveness/agent-tab-liveness`, base `master`

## Responsibility

The liveness rule and everything that reads it: `LiveSessions` (`tddy-daemon-kernel`),
`CliSessionManager::live_agent_sessions`, `SandboxSessionManager::live_sessions` /
`SandboxSessionState::is_running`, `list_sessions_in_dir(base, &LiveSessions)` and every one of its
callers; delete, SignalSession / TerminateSession and the project-home idle reaper acting only on
processes the daemon holds (the `tool` carve-out excepted).

## Boundaries

- No per-tab roster in `.session.yaml`, no tmux-wrapped agent tabs, no eager re-attach at startup —
  `feature/session-liveness/tmux-roster` (2/4).
- No `tool`-session process tab and no per-backend exit signal — `feature/session-liveness/tool-tab`
  (3/4). Tool sessions list inactive here and keep their recorded-pid delete/signal.
- No restoring of tabs on Resume — `feature/session-liveness/resume-roster` (4/4).

## Dependencies

None — root node, based on `master`.

## Draft PR contract

The first push publishes `LiveSessions` (implemented — a value type), the `todo!()` signatures of
`CliSessionManager::live_agent_sessions`, `SandboxSessionManager::live_sessions` and
`SandboxSessionState::is_running`, and the failing acceptance and unit tests listed below. The
`list_sessions_in_dir` signature change is this node's green work (its unit tests are written
against it).

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — every test drives this node's own surface over existing RPCs
**Concurrent with:** none
**Blocks:** `tmux-roster` (2/4: a re-attached tab must read live by this rule), `tool-tab` (3/4: a
tool tab becomes live through this listing), and `resume-roster` (4/4) through 2/4

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3      n2 → n4

## Affected Packages

- **tddy-daemon-kernel**: [README.md](../../../packages/tddy-daemon-kernel/README.md) — new `live_sessions::LiveSessions`
- **tddy-session-activity**: [README.md](../../../packages/tddy-session-activity/README.md) — `list_sessions_in_dir` takes the live set; delete stops held processes only
- **tddy-cli-sessions**: [README.md](../../../packages/tddy-cli-sessions/README.md) — `CliSessionManager::live_agent_sessions`
- **tddy-daemon-sandbox**: [README.md](../../../packages/tddy-daemon-sandbox/README.md) — `SandboxSessionManager::live_sessions`, `SandboxSessionState::is_running`; no pid fallback in `stop`
- **tddy-agent-launch**: [README.md](../../../packages/tddy-agent-launch/README.md) — listing, ensure-home, idle reaper, signal, delete, PR-status owner lookup
- **tddy-session-lifecycle**: [README.md](../../../packages/tddy-session-lifecycle/README.md) — acceptance tests; host exposes the live set
- **tddy-daemon-rpc**, **tddy-worktree-service**, **tddy-telegram-control**: pass the live set through

## Related Feature Documentation

- PRD: [PRD-2026-11-19-session-liveness-from-agent-tabs.md](../../ft/daemon/1-WIP/PRD-2026-11-19-session-liveness-from-agent-tabs.md)
- [session-terminal-tabs.md](../../ft/web/session-terminal-tabs.md), [inactive-session-activities.md](../../ft/web/inactive-session-activities.md), [project-home-auto-stop.md](../../ft/daemon/project-home-auto-stop.md)
- Discovery: [2026-11-19-session-liveness-from-agent-tabs-initial-discovery.md](2026-11-19-session-liveness-from-agent-tabs-initial-discovery.md)

## Summary

Session liveness is decided by what the daemon holds — at least one live agent tab — instead of
`kill(pid, 0)` on the pid recorded in `.session.yaml`, which reads a recycled pid as a live session.
Delete, SignalSession and the idle reaper stop acting on the recorded pid for the same reason.

## Background

See the PRD. Root cause verified at `packages/tddy-session-activity/src/session_reader.rs:50,90`;
no production code clears `pid`.

## Prerequisites

### ✅ RESOLVED HERE — `EnsureProjectHomeSession` decides "running" by pid liveness or a served terminal — [2026-10-23-project-home-running-heuristic.md](../todo/2026-10-23-project-home-running-heuristic.md)

Its fix is this change: `running` comes from the live set alone, so a main with a live foreign pid and
no served terminal is resumed. The acceptance case it asks for is
`ensuring_a_main_whose_recorded_pid_names_an_unrelated_process_resumes_it`. The *Replace main* half
(a main that outlived a restart is left running) is **not** closed: after a restart nothing holds it,
so nothing can stop it without signalling a pid. Narrow the entry at wrap; node 2's eager re-attach
is what makes such a main held again.

### ⚠ DURING — Project-home auto-stop cannot stop an untracked session process — [2026-10-23-project-home-auto-stop-untracked-processes.md](../todo/2026-10-23-project-home-auto-stop-untracked-processes.md)

An untracked session is no longer active, so the reaper skips it up front (no 5 s wait, no repeated
warning). Its process is still not stopped — that needs a pid-verified stop, which this change
deliberately does not add. Narrow the entry at wrap.

### Code issues

`packages/tddy-agent-launch/docs/code-issues/complexity-remote-agent-launch-launch-on-target.md` —
unrelated (target launch; not touched). No other code issue names the touched symbols.

## Scope

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW — approved 2026-11-19
- [x] TDD Red — failing unit/integration tests
- [ ] **Implementation**: live set + every listing caller + delete / signal / reaper
- [ ] **Testing**: acceptance + unit tests below passing; old pid-faking tests reworked
- [ ] **Interim TODO**: `tool` sessions inactive + yaml-pid signal carve-out, naming node 3
- [ ] **Package Documentation**: READMEs / dev docs updated at wrap
- [ ] **Code Quality**: scoped clippy for every touched crate

## Technical Changes

### State A (Current)

- `list_sessions_in_dir(sessions_base)` → `SessionEntry.is_active = metadata.pid.map(is_pid_alive)`.
- `DaemonSessionListing` (unit struct) → `SessionClaim.is_active` from the same listing.
- `ensure_on_this_host`: `is_running = main.is_active || cli_manager.get(id).is_some()`.
- Idle reaper: lists `is_active` sessions, stops via `cli_manager.stop_session`, then polls
  `kill(metadata.pid, 0)`.
- Delete: `terminate_session_process(metadata.pid)`; tabs are never stopped.
- SignalSession / TerminateSession: `kill(metadata.pid, sig)` after `kill(pid, 0)`.
- `SandboxSessionManager` keeps an entry after its runner exits; `SandboxSessionState::stop` falls
  back to signalling `self.pid` once its handle is gone.

### State B (Target)

- `tddy_daemon_kernel::live_sessions::LiveSessions` — the set of session ids with ≥ 1 live agent
  tab: `LiveSessions::none()`, `LiveSessions::of(ids)`, `contains(&str)`, `union(other)`.
- `CliSessionManager::live_agent_sessions().await -> LiveSessions` — every session with a resolving
  terminal whose kind is neither `"bash"` nor `"tmux"`, plus every session a tmux detach watcher
  holds.
- `SandboxSessionManager::live_sessions().await -> LiveSessions` — entries whose
  `SandboxSessionState::is_running()` (its owned `Child` has not exited: `try_wait() == None`).
- `list_sessions_in_dir(sessions_base, &LiveSessions)` — `is_active = live.contains(session_id)`.
  The parameter is required; no caller can omit it, and there is no pid fallback.
- `DaemonSessionListing::new(live)`.
- Every caller takes the live set from the daemon's own `CliSessionManager` ∪
  `SandboxSessionManager` (one helper on the host / `LaunchSessions`; `from_host` handlers and the
  Telegram harness are handed it).
- Ensure-home: `is_running = live.contains(main)`. Reaper: waits for the session to leave the live
  set, not for a pid.
- Delete: `cli_manager.stop_session` + sandbox stop; the recorded pid is signalled only for
  `session_type == "tool"` (interim, node 3).
- SignalSession / TerminateSession: the held main's pid (`PtyHandle.pid` / the sandbox state's
  owned child); refused when none is held. `tool`: recorded pid (interim, node 3).
- `SandboxSessionState::stop` never signals a bare pid.

### Session kinds (PR 1)

| Kind | Live while |
|---|---|
| PTY main (local, project-home, orchestrator, split agent half) | its `main` terminal resolves |
| tmux-run main | its client `main` resolves, or a detach watcher holds it; after a restart, once re-attached |
| target (ssh / Coder) | its transport-client `main` resolves (it exits with the remote agent) |
| sandboxed | its runner's `Child` has not exited |
| agent tab | its terminal resolves |
| bash / tmux-attach tab | never counts |
| `workspace` | never (no agent) — unchanged |
| `tool` | **never, until node 3** |

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/session_liveness_acceptance.rs`:

- [ ] `a_session_whose_recorded_pid_belongs_to_an_unrelated_live_process_lists_inactive`
- [ ] `a_session_whose_main_agent_runs_lists_active_without_a_recorded_pid`
- [ ] `a_session_whose_main_agent_exited_while_an_agent_tab_runs_lists_active`
- [ ] `a_session_whose_agent_tabs_have_all_closed_lists_inactive`
- [ ] `a_session_with_only_a_shell_tab_lists_inactive`
- [ ] `a_session_lists_inactive_after_the_daemon_restarts`
- [ ] `deleting_a_session_leaves_the_process_its_recorded_pid_names_running`
- [ ] `deleting_a_session_stops_its_agent_tabs`
- [ ] `signalling_a_session_reaches_its_main_agent_not_the_recorded_pid`
- [ ] `signalling_a_session_with_no_running_agent_is_refused_and_signals_nothing`

`packages/tddy-session-lifecycle/tests/claude_cli_tmux_reattach_acceptance.rs`:

- [ ] `a_tmux_run_agent_that_outlived_its_daemon_lists_inactive_until_its_main_is_reattached`
- [ ] `a_tmux_run_agent_whose_client_detached_lists_active_while_the_daemon_watches_it` — passes
  today through the pane pid; a guard that the new rule keeps a watched detached agent live

`packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs`:

- [ ] `ensuring_a_main_whose_recorded_pid_names_an_unrelated_process_resumes_it`

## Unit Tests

- `tddy-daemon-kernel` `live_sessions` (implemented — a plain value type): membership, `none`, `union`.
- `tddy-session-activity` `session_reader`: a live recorded pid outside the set lists inactive; a
  session in the set without a pid lists active. Written against the new required parameter, so
  this crate's unit-test target does not compile until green changes the signature.
- `tddy-cli-sessions` `cli_session_manager/live_agent_sessions_tests.rs`: main agent counts; agent
  tab counts; shell does not; a stopped tab and a tab that exited on its own drop out. (A tmux-attach
  tab is excluded by the same kind rule as the shell; its spawn runs as an OS user, so it is not
  unit-tested here.)
- `tddy-daemon-sandbox` `sandbox_session_liveness_tests.rs`: a running runner counts; an exited one
  does not, though its entry remains.

Contract stubs (`todo!()`, `TODO(session-liveness 1/4)`): `CliSessionManager::live_agent_sessions`,
`SandboxSessionManager::live_sessions`, `SandboxSessionState::is_running`.

## Tests to rework (they specify the old rule)

`tests/session_branch_conflict_acceptance.rs` and
`tddy-telegram-control/tests/telegram_branch_conflict_acceptance.rs` fake an active owner with
`std::process::id()` as its pid; `tests/claude_cli_tmux_session_acceptance.rs:345` and
`tests/project_home_idle_stop_acceptance.rs` read `is_active` of real sessions and should keep
passing. Green reworks the pid-faking ones to hold a live terminal instead.

## Decisions & Trade-offs

- **Liveness is per daemon.** A session is active on the daemon that holds its agent; nothing reads
  another host's processes.
- **No fallback** to the recorded pid anywhere in listing. The `tool` carve-out is in delete/signal
  only, explicit, and TODO-marked for node 3 (developer decision, 2026-11-19).
- **A detached tmux agent counts** while this daemon's watcher holds it — the daemon tracks it.
- **Roster deferred to node 2**, with its first reader (developer decision).
- **Eager tmux re-attach deferred to node 2**; until then a restart leaves a surviving tmux agent
  inactive until re-attached (developer decision).
