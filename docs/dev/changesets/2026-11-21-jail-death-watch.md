# 2026-11-21 — A dead main jail ends its session's terminal

**Type:** Bug Fix · **Stack:** `#tab-hardening` 3/4 (PR #991) · **Packages:** `tddy-daemon-sandbox`,
`tddy-session-lifecycle`, `tddy-agent-launch` · Feature: [session-liveness.md](../../ft/daemon/session-liveness.md#a-dead-main-jail)

## Problem

Nothing watched a sandboxed session's main runner. Its terminal never reported done (`pty_done` followed the stdout
broadcast closing, which the jail's state kept open for as long as it lived), the exit status was lost to the first
`is_running()` poll (`Child::try_wait` reaps), the relay and endpoint tasks were detached, and the registry kept a
dead jail until a resume or delete replaced it.

## What changed

- **`jail_exit_watch`** (new module, `tddy-daemon-sandbox`): `watch_jail_exit` waits for the runner with
  `waitid(WEXITED | WNOWAIT)` on a blocking task — no lock held, the state held weakly — and records the `ChildExit`
  on the state. `SandboxSessionManager::insert` starts it; a jail that exits on its own leaves the registry (when it
  is still the registered one, finishing its capture recording) and is announced on `subscribe_ended` as a
  `JailEnded`. `stop` marks `stopped` before it kills, so a stop by request is never announced.
- **`JailProcess`** (`Owned(Child)`) behind `SandboxSessionState`: `try_exit` reads with `WNOHANG | WNOWAIT`, so
  `is_running` / `runner_pid` never reap the status away from the watch; `kill_and_reap` is what stop and the watch
  end with. A supervised jail (jail-uid) is a later variant.
- **`JailBridge`**: `dial_and_bridge` / `dial_and_bridge_with_egress` return the endpoint, relay and forwarder
  `JoinHandle`s, the launch paths attach them with `SandboxSessionState::with_bridge`, and stop aborts them.
- **The terminal follows the exit**: `SandboxTerminalSession::subscribe_pty_done` follows
  `SandboxSessionState::subscribe_exit`, so the main's stream ends when the jail dies or is stopped.
- **Tests**: `jail_exit_watch_tests` (status for `exit 3` and for a signal, a silent stop, the status kept when
  liveness was polled first, the bridge aborted), the liveness tests, and the parity suite's
  `a_sandboxed_sessions_terminal_ends_when_its_jail_dies` and
  `a_sandboxed_session_whose_jail_died_lists_inactive_and_keeps_its_tab_jails`. The stand-in jail built three times
  in the crate is one (`jail_test_support`), and the lifecycle suites share `a_stand_in_runner`.
- **Restructuring (file-length gate)**: `sandbox_session.rs` grew 938 → 1,037 production lines; three mechanical
  moves, re-exported from `sandbox_session` so no path changed, take it to **416**: `jail_host_bridge.rs` (360 —
  `bridge_sandbox_stdio`, `DaemonToolHandler`, `connect_sandbox_session_client`, `dial_and_bridge*`,
  `forward_jail_terminal_events`), `runner_paths.rs` (146 — binary and read-path discovery) and `runner_spawn.rs`
  (144 — `SandboxRunnerSpawn`, `build_sandbox_plan`, the spawn functions). Moved by hand with line ranges and proven by
  the crate's tests: the restructure engine was not run, because its cold index load does not fit this contended host.

## Decisions & Trade-offs

- **End like a terminal, resumable; tab jails untouched** (developer decision). The session is inactive because its
  live set reads the runner; no status is written to `.session.yaml`.
- **Wait without reaping**, so the status is never lost to `is_running()`.
- **jail-uid is split out** and recorded as
  [a todo](../todo/2026-11-21-jails-run-as-the-daemons-uid.md); the main jail's death watch narrows the
  [shutdown todo](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md), which stays for the
  workspace jail and a restart policy.

## Verification (scoped)

`tddy-daemon-sandbox` lib 53/53; `tddy-session-lifecycle` lib 189/189 and the parity, shell-tab, agent-tab,
shutdown and resume-roster suites; `tddy-agent-launch` lib 382/382; scoped `clippy -D warnings` for
`tddy-daemon-sandbox` (all targets) and `tddy-agent-launch`. Full-workspace health is CI's.
