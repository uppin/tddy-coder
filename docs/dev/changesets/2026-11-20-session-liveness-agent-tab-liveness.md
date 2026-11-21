# 2026-11-20 — A session is live only while the daemon holds an agent tab

**Type:** Fix · **Stack:** `#session-liveness` 1/4 (PR #958) · **Packages:** `tddy-daemon-kernel`,
`tddy-session-activity`, `tddy-cli-sessions`, `tddy-daemon-sandbox`, `tddy-agent-launch`,
`tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-worktree-service`, `tddy-telegram-control`,
`tddy-daemon` · Feature: [session-liveness.md](../../ft/daemon/session-liveness.md) · Backlog:
`2026-10-23-project-home-running-heuristic` and `2026-10-23-project-home-auto-stop-untracked-processes`
(both narrowed, not closed)

## Problem

Closed sessions reappeared as **active** in the web drawer (`01a1e86c-…`: Status `active`, PID 22059,
Activity `Ended`). Liveness was `kill(pid, 0)` on the pid in `.session.yaml`
(`session_reader::list_sessions_in_dir`); nothing clears that pid when the agent exits, so a recycled
pid made a dead session live again. Delete, `SignalSession`, `TerminateSession` and the project-home
idle reaper also *signalled* or waited on that pid, so a delete could kill an unrelated process.

## What changed

- **`tddy_daemon_kernel::live_sessions`**: `LiveSessions` (`none`, `of`, `contains`, `union`) is a
  snapshot of the sessions the daemon holds a live agent for; `HeldSessions` (`live_sessions`,
  `stop_session`) is the port for surfaces built before the host, with `NoHeldSessions` and the
  late-bound `LateBoundHeldSessions` (bound in `install_session_host` → `bind_late_bound_ports`).
- **Holders**: `CliSessionManager::live_agent_sessions()` — a resolving `main` or agent-kind terminal
  (never `"bash"` / `"tmux"`) plus every session a tmux detach watcher holds, and the `settling` hold
  between a tmux client's end and the watcher taking over;
  `SandboxSessionManager::live_sessions()` over `SandboxSessionState::is_running()`, read off the owned
  `Child`; `SandboxSessionState::stop` no longer signals a bare pid.
- **`list_sessions_in_dir(sessions_base, &LiveSessions)`**: `is_active = live.contains(id)`, a required
  parameter with no pid fallback; `DaemonSessionListing::new(live)`. Every caller reads the set from the
  daemon's managers (`LaunchSessions::live_sessions`, `DaemonTerminalSessionStore::live_sessions`,
  `PackExecutionContext::live_sessions`, Telegram's `with_held_sessions`).
- **Delete, SignalSession, TerminateSession, the idle reaper, ensure-home**: delete stops the session's
  terminals and sandbox (the recorded pid is signalled only for `tool`); `signal_held_session` signals
  the held main's pid and refuses with `session has no running agent` when none is held; the reaper
  waits for the session to leave the live set; ensure-home's `running` is the live set alone.
- **`ReattachOutcome`** (`tddy-cli-sessions`): every main re-attach attempt announces how it ended on
  `reattach_finished()` and under the `tddy_cli_sessions::reattach` log target.
  `reattach_recorded_main` returns it.

## Wrap cleanups

- `reattach_recorded_main` split into `reattach_when_recorded`, `require_live_tmux_session` and
  `recorded_cli_agent` (80 → 16 + 42 + 14 + 24 lines).
- `spawn_terminal_cleanup` (54 → 74 in the green, 57 after): the settle logging moved into
  `DetachWatch::{settling, settled}` and the end announcement into `ended_agent_announcement`.
- `install_session_host` (59 → 63 in the green, 51 after): the late-bound port binding moved into
  `bind_late_bound_ports`.
- The `terminal_manager` accessor no longer sits between `terminal_sessions` and its doc comment, and
  the recycled-pid test helper moved out of the shared `ensure_project_home_world` module into its one
  suite, so no `#[allow(dead_code)]` was added.

## Code issues

- **Deleted:** `tddy-agent-launch` `complexity-claude-cli-tmux-reattach-recorded-main` — final
  measurement: `reattach_recorded_main` 80 lines (76 at detection) → 16, the longest helper 42.
- **Added, not fixed (stack-shared files, developer's rule):** `oversized-file-svc-signal-delete-session`
  (446 → 511) and `oversized-file-test-util` (484 → 578); four existing `oversized-file` records got a
  measurement row (`sandbox_session.rs` 885 → 924, `pr_stack/ports.rs` 878 → 880,
  `chaining_and_listing.rs` 521 → 527, `pickers.rs` 627 → 628). Backlog:
  `2026-11-20-session-liveness-oversized-files-after-the-stack`.
- **Not measured:** `tddy-tools analyze coverage` did not run (ten instrumented crate builds on a disk
  shared with two other builds), so CRAP scores were not re-derived; function length was measured
  against the merge base instead.

## Tests

`tddy-session-lifecycle/tests/session_liveness_acceptance.rs` (ten cases), the two re-attach cases of
`claude_cli_tmux_reattach_acceptance`, and
`ensure_project_home_session_acceptance::ensuring_a_main_whose_recorded_pid_names_an_unrelated_process_resumes_it`;
unit: `live_sessions`, `session_reader`, `live_agent_sessions_tests`, `sandbox_session_liveness_tests`,
`signal_session_unit_tests`, the remote-git pack resolver. The pid-faking suites
(`session_branch_conflict_acceptance`, `telegram_branch_conflict_acceptance`, the project and terminate
suites) hold a live terminal instead.
