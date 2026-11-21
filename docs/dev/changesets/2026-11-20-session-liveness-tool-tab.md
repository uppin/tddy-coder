# 2026-11-20 — A tool session's tddy-coder is a process tab, live until its spawn backend reports the exit

**Type:** Bug fix · **Stack:** `#session-liveness` 3/4 (PR #960) · **Packages:** `tddy-task`, `tddy-spawn`,
`tddy-supervisor`, `tddy-cli-sessions`, `tddy-agent-launch`, `tddy-session-activity`,
`tddy-session-lifecycle` · Feature: [session-liveness.md](../ft/daemon/session-liveness.md) ·
Backlog: `2026-11-19-tool-sessions-liveness-and-pid-signals` (closed here),
`2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable` (measurement row added)

## Problem

A running tool session listed inactive, because the daemon held nothing for its `tddy-coder` (the
supervisor, the spawn worker or a detached wait thread did), and delete and signal still went to the pid
its `.session.yaml` records — a pid the OS may have reissued.

## What changed

- **`tddy_task::child_exit`**: `ChildExit`, `ExitSender` / `ExitReceiver`, `exit_channel` and `HolderGone`.
  In `tddy-task` so the terminal registry does not depend on the spawn backends; `tddy-spawn` re-exports them.
- **`tddy_spawn::child_exit`**: `SpawnedTool { result, exit }`, one exit report per backend — the direct
  spawner's wait thread (`reap_in_background`), `WorkerEvent::Exited{pid, code, signal}` written by the worker
  (`SpawnClient::spawn_tool`), and the daemon polling the supervisor's `SessionStatus`
  (`spawn_tool_via_supervisor`, `watch_supervised_exit`).
- **Supervisor `SignalSession(SessionRef, signal) -> SessionStatus`** and `SupervisorClient::signal_session`:
  signals the pid alone, refused for a pid it never spawned, an exited session reported not signalled.
- **`CliSessionManager` process tab** (`register_process_tab`, `process_tab`, `signal_process_tab`): a PTY-less
  `main` tab of kind `tddy-coder`, counted by `live_agent_sessions`, absent from `list_terminals`, removed on
  exit; `stop_session` asks its holder to stop it.
- **`tddy-agent-launch`**: `tool_session_spawned` registers the tab before the spawn returns
  (`tool_process_tab.rs`: the supervisor and owned-child holders, the ended flag); `watch_until_stopped`
  waits on that flag instead of probing the pid; the `tool` carve-out of `1/4` is gone from
  `signal_held_session` and `delete_session_directory`.

## Wrap cleanups

- `svc_signal_delete_session.rs` 538 -> 384 production lines (engine-driven `move_item` +
  `move_impl_members`): the signal half is `svc_signal_session.rs` (159), `signal_held_session` keeps its path
  through a `pub use`. Its oversized-file record is deleted on that measurement (446 -> 511 in #958, 538 here);
  its row left `docs/dev/todo/2026-11-20-session-liveness-oversized-files-after-the-stack.md`, because #961
  does not edit it.
- Functions this PR grew past 60 lines, shortened: `spawn_tddy_coder` (85 -> 10, one function per backend),
  `run_spawn` (69 -> 46), `clone_repo` (62 -> 28), `spawn_as_user_reporting` (the former `spawn_as_user`,
  195 -> 182, the wait thread is `reap_in_background`). `spawn_tool_session` (142 -> 141) and
  `resume_session_at_session_coordinate` (191 -> 190) shrank and keep their open records, with a row each.
- `spawner.rs` grew 2,152 -> 2,188 lines (the forwarding `spawn_as_user` and the non-Unix stub); recorded in
  the tddy-spawn over-budget entry.

## Known gaps

- Forked-worker and direct backends: between the kernel reaping the child and the exit report reaching the
  daemon a recycled pid could be signalled. The supervisor backend has no such window.
- A plain `spawn()` through the worker now receives an `Exited` event nobody waits on; it is dropped at debug level.
- The Linux-only supervisor tests run on CI, not on the macOS dev machine.
