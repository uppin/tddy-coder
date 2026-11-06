# Bounded session waits — PRD

**Date**: 2026-11-06
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 23/28 — branch `feature/agent-tools/bounded-waits`

## Affected Features

- [tddy-supervisor.md](../../supervisor/tddy-supervisor.md) — calls from the daemon to the supervisor
- [session-agent-roster.md](../session-agent-roster.md) — starting a session
- [clone-ssh-passphrase.md](../clone-ssh-passphrase.md) — the askpass clone path

## Summary

The rule for this stack is "the host is mandatory for runners; nobody hangs". After this change,
every wait on the session-start path has a deadline that fails with a named error: each supervisor
call, the supervisor clone stream, and the generic session start itself. The reported
`clone_askpass` host-key hang is reproduced and either fixed or recorded with evidence.

## Background

- [2026-11-04-acp-launch-waits-have-no-deadline.md](../../../dev/todo/2026-11-04-acp-launch-waits-have-no-deadline.md):
  the ACP `open_session` half is already bounded (120 s). The supervisor half is not: `SupervisorClient::call`
  awaits `call_unary` with no deadline, and so does `clone_repository`'s stream and its first frame.
- [2026-11-06-generic-session-start-has-no-deadline-of-its-own.md](../../../dev/todo/2026-11-06-generic-session-start-has-no-deadline-of-its-own.md):
  the start runs under a fresh `CancellationToken` that never fires. Only Telegram has an outer bound
  (600 s). The web RPC and peer RPC have none.
- The `clone_askpass` host-key acceptance test was seen hanging against a real sshd. The code has a
  60 s test guard, and the cause is unconfirmed.

## Proposed Changes

### What's Changing

- Every `SupervisorClient` call is bounded. On expiry it returns `SupervisorError::Unavailable`
  naming the method and the deadline. The clone stream is bounded until its first frame, and
  between frames by a progress deadline.
- A session start has a deadline of its own, configurable in `daemon.yaml` next to
  `spawn_worker_request_timeout_secs`. On expiry the start's token is cancelled, its partial work
  is undone the way a cancelled start already is, and the caller gets `DeadlineExceeded` naming the
  step that was running.
- Telegram's own 600 s bound is replaced by the start deadline. Its chat message keeps working.
- The host-key askpass path is run against the real sshd. If it hangs, the fix makes the
  confirmation fail fast.

### What's Staying the Same

- The ACP `open_session` deadline (120 s) and the SSH slot-write deadline.
- What a successful start does.

## Impact Analysis

### Technical Impact

`tddy-supervisor` (client), `tddy-agent-launch` (start, clone planner if the hang is real),
`tddy-daemon-kernel` (config knob), `tddy-telegram-control` (drop its own deadline).

### User Impact

A start that cannot finish fails within the configured time, with the step named, instead of
spinning forever in the UI.

## Implementation Plan

1. Supervisor call and clone-stream deadlines (silent-peer test).
2. Start deadline knob plus cancellation (never-returning start-step fake).
3. Reproduce the host-key test; fix or record it.

## Acceptance Criteria

- [ ] A supervisor that accepts and never answers makes `spawn_relayed` fail with `Unavailable` naming the method within the deadline.
- [ ] A supervisor clone that never sends a first frame fails within its deadline.
- [ ] A session start whose step never returns fails with `DeadlineExceeded` naming the step, and its partial session is cleaned up.
- [ ] `session_start_timeout_secs: 0` is clamped like `spawn_worker_request_timeout_secs`.
- [ ] The host-key confirmation clone fails in seconds against the real sshd, or a record states what was run and seen.

## References

### Affected Features (Complete List)

- `docs/ft/supervisor/tddy-supervisor.md`
- `docs/ft/daemon/session-agent-roster.md`
- `docs/ft/daemon/clone-ssh-passphrase.md`

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-11-06-bounded-session-waits.md`
