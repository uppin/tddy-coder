# Session-creation progress you can watch, and cancel — umbrella PRD

**Date**: 2026-10-24
**PRD Type**: Feature Enhancement + Bug Fix
**Stack**: `#start-progress`, 8 nodes plus a sweep node. This umbrella lives on the root node
(`feature/start-progress/spawn-diag`); each node carries its own PRD for its slice.

## Affected Features

- **Primary**: [Remote-managed worktree](../remote-managed-worktree.md) (split sessions, § Failure is atomic),
  [Project concept](../project-concept.md) (where a clone happens),
  [Cloning over SSH](../clone-ssh-passphrase.md) (forwarded-start budget, keep-alive)
- [Session drawer](../../web/session-drawer.md) (§ Start progress — the create pane)
- [systemd install](../systemd-install.md) and [tddy-supervisor](../../supervisor/tddy-supervisor.md)
  (how the daemon learns it is supervised)

## Summary

Creating a session — locally or with the codebase on another host — shows what the host is doing
**while it does it**: cloning the repository (git's own stage, percent, bytes and throughput), cutting
the worktree, building the index, starting the agent. Nothing on that path gives up on a timer: a
start ends when it succeeds, fails, its host goes silent, or the operator presses **Cancel**. When a
clone or spawn stalls, the daemon log says exactly where.

## Background — the 2026-10-24 incident

A split claude-cli start (agent on the desktop, codebase on `mantas-udoo-ros`) failed after 30 s with
no visible progress. Four defects lined up:

1. The remote daemon's `daemon.yaml` predates the supervisor, so it lacked the `supervisor:` block and
   cloned through its own unprivileged forked worker, which can never become another OS user.
2. The clone child deadlocked before `exec` (`std::env::set_var` inside `pre_exec` while `std` holds
   the environment lock across `fork`), instead of failing with a permission error. Nothing logged
   where it stuck.
3. The peer's own 30 s clone timeout fired; the desktop misread the peer's `DEADLINE_EXCEEDED` as its
   own forward deadline. The peer logged nothing.
4. The forked worker stayed wedged behind the hung child for every later request.

And the create pane showed nothing: the split start forwards a unary `StartSession`, the peer reports
into a discarding sink, and no clone reports progress at all.

## Proposed Changes — by node

| K | Node | Delivers |
|---|---|---|
| 1 | `spawn-diag` | A spawn or clone that cannot switch user fails at once with a message saying why; one that stalls is logged step by step (fork → groups → gid → uid → exec) with a periodic "still at step" line. The pre-exec deadlock is gone |
| 2 | `worker-mux` | The forked spawn worker serves requests concurrently and can stream events for one request; a long clone no longer blocks session spawns |
| 3 | `supervisor-handoff` | A supervised daemon uses its supervisor because the supervisor told it so; the `supervisor:` config block is gone and `./install` removes it on upgrade |
| 4 | `split-stream` | A split start streams the codebase host's progress through to the operator, with no overall deadline; every hop keeps the stream alive |
| 5 | `clone-progress` | A start that clones reports a **Cloning** step with git's progress; the clone has no deadline, and a failed clone leaves no half-written repository |
| 6 | `supervised-clone-progress` | The same clone progress, and the clone's own error text, on a supervised host |
| 7 | `progress-ui` | The create pane shows a step list with a live clone progress bar |
| 8 | `start-cancel` | **Cancel** stops a start in progress on every host involved and removes what it had created |
| — | sweep | Closes every code issue the stack added; open TODOs go to the developer |

## What's Staying the Same

- A start's result is unchanged: one session, its room and its worktree, or an error.
- The 30-second relay idle timeout on a forwarded stream stays; it is what ends a start whose peer has
  gone silent. Every hop now sends keep-alives, so it never fires on a live start.
- `spawn_worker_request_timeout_secs` keeps its meaning for everything other than session creation
  (worktree-service RPCs, session spawns, session-file reads, PR-stack ports).
- CreateProject / AddProjectToHost stay unary (their clones lose the deadline and gain the
  diagnostics; streaming them is recorded as a TODO).

## Impact Analysis

- **Technical:** `session.proto` gains a clone step, a clone-progress event and a start-task event;
  `supervisor.proto` gains a streaming clone RPC; the forked worker protocol changes (daemon and worker
  are one binary, so there is no version skew); `DaemonConfig` loses a key.
- **Operator:** an existing `daemon.yaml` with `supervisor:` fails to start with a message naming the
  block; `./install --systemd` deletes it.
- **User:** session creation is observable and cancellable; failures name their cause.

## Success Criteria (stack-wide)

- [ ] The incident's shape — a pre-supervisor `daemon.yaml` on a supervised host, a split start that
      clones — starts the session, with clone progress visible in the create pane.
- [ ] A start that cannot proceed fails with the cause in the error and in the daemon log, never after
      an unexplained timer.
- [ ] Cancel ends a start within seconds on every host, leaving no session directory, worktree or
      partial clone behind.

## Out of Scope

Streaming CreateProject / AddProjectToHost; a cancellation surface on `RpcService`; client-side
deadlines for LiveKit RPC; the peer `ListProjects` fan-out's 3 s skip.
