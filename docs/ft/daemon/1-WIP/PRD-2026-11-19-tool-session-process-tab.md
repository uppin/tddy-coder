# Tool sessions as a process tab - PRD

**Date**: 2026-11-19
**Status**: Draft
**Stack**: `#session-liveness` 3/4 — `feature/session-liveness/tool-tab`
**Changeset**: [2026-11-19-tool-session-process-tab.md](../../../dev/1-WIP/2026-11-19-tool-session-process-tab.md)

## Affected Features

- [Terminal sessions](../terminal-sessions.md) — the session's tabs
- [Background tasks](../background-tasks.md) — `TaskRegistry`
- [Supervisor](../../supervisor/) — `SessionStatus`

## Summary

A `tool` (tddy-coder) session's process becomes a **process tab** of its session in the same
registry the terminal tabs use. It is live until the spawn backend that holds the child reports that
the child exited. Its session lists active while it runs, and delete, SignalSession and
TerminateSession reach it through that tab instead of the pid its `.session.yaml` records. This
closes the carve-out `#session-liveness` 1/4 left for tool sessions.

## Background

After 1/4 a running tool session lists **inactive**: the daemon holds nothing for it. Its child is
held by `tddy-supervisor`, the forked spawn worker, or a detached `child.wait()` thread, and the
daemon gets only `SpawnResult.pid`. Delete and signal still use the recorded pid, so the
recycled-pid hazard remains for tool sessions. Developer decision: a tool session is "just a
different type of a tab".

## Proposed Changes

### What's Changing

- **Exit reported by whoever holds the child**, as one daemon-side value per spawn, "exited, with
  this status":
  - **direct spawn** — the existing wait thread sends the status on a channel instead of only logging it;
  - **forked spawn worker** — a new `WorkerEvent::Exited{pid, code, signal}` on the worker's existing
    event stream, sent by the wait thread inside the worker;
  - **supervisor** — the daemon polls `SessionStatus(SessionRef{pid})`. The supervisor owns the
    child, so its `EXITED` is exact; it is not a pid probe.
- **A process tab.** The tool child is registered under its session as a tab of kind `tddy-coder`,
  with id `main`. It is a task with no PTY. It counts as an agent for liveness, and it is not listed
  by `ListTerminalSessions` (it has no terminal to stream — the session's UI is tddy-coder's own,
  over LiveKit/gRPC). It leaves the registry when its exit is reported.
- **Stopping and signalling.** Delete stops the process tab through its holder: `StopSession` on the
  supervisor, a signal to the child the worker or direct thread owns. SignalSession and
  TerminateSession deliver to the held child. With no tab held they are refused (`process is not
  alive`), as for every other kind. The 1/4 `session_type == "tool"` carve-out is removed.
- `HostSessionSockets::watch_until_stopped` stops polling the pid and unregisters the socket when the
  tab's exit is reported.
- **Resume** of a tool session registers the respawned child the same way.

### What's Staying the Same

- How tddy-coder is spawned, and what it writes into its own `.session.yaml`.
- After a daemon restart a tool session is inactive. Whether the child outlived the daemon depends
  on the backend, and the daemon holds nothing for it.

## Acceptance Criteria

1. A running tool session lists active, for each spawn backend (direct, forked worker, supervisor).
2. A tool session whose tddy-coder exited lists inactive, for each backend.
3. Deleting a tool session whose recorded pid names an unrelated process stops its tddy-coder and
   leaves that process running.
4. SignalSession on a running tool session reaches its tddy-coder; on a stopped one it is refused and
   signals nothing.
5. The worker reports `Exited` with the child's exit code, and the direct spawner's channel carries it.
6. A resumed tool session is live again.
