# A stalled or impossible spawn says why — PRD

**Date**: 2026-10-24
**PRD Type**: Bug Fix
**Stack**: #start-progress 1/8 (base `master`)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) (the clone a start runs as the session's OS user)
- **Related Feature**: [systemd install](../systemd-install.md) — the unprivileged daemon under the supervisor

## Summary

Every child the daemon starts as another OS user — a session, a clone, a host probe — either becomes
that user or fails within moments with a message saying why. While one is being started, the daemon
log records each step it passes (forked, supplementary groups set, gid set, uid set, exec'd) with the
child's pid, and if a step does not finish, a line every few seconds names the pid, the step and how
long it has been there.

## Proposed Changes

### What's Changing
- The deadlock is removed: no child touches the process environment between `fork` and `exec`, and
  supplementary groups are resolved before the fork (no NSS lookup in the child).
- A process that is not root and is asked to start a child as a **different** user refuses before
  forking, naming the user it runs as, the user requested, and that a supervised host brokers such
  spawns through `tddy-supervisor`.
- Each child reports its pre-exec steps to the parent; the parent logs them and a "still at step"
  line while one is outstanding.
- A clone that fails or exceeds its deadline on the session-start path is logged on the host doing the
  clone, with the destination and the cause (today the timeout returns silently).

### What's Staying the Same
Which backend runs a spawn; the clone's command and environment; deadlines (removed by later nodes).

## Acceptance Criteria
- [ ] A clone or spawn as another user from a non-root process fails immediately with a message naming
      both users; no child is forked.
- [ ] A spawn that execs logs its steps in order, ending at exec, with its pid.
- [ ] A child held before exec produces a "pid N still at step S after Ns" line.
- [ ] No `pre_exec` closure in the workspace calls `set_var`, `initgroups` or allocates.
- [ ] A session-start clone that fails or times out is logged on the cloning host.

## Out of Scope
Making an unprivileged daemon able to switch user (that is the supervisor's job, node 3); clone progress.
