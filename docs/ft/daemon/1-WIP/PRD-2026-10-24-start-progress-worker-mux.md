# The spawn worker serves requests concurrently — PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: #start-progress 2/8 (base `feature/start-progress/spawn-diag`)

## Affected Features

- **Primary Feature**: [systemd install](../systemd-install.md) § `--user` and the desktop app — the hosts
  that spawn through the daemon's forked worker rather than the supervisor

## Summary

On a host without a supervisor (Tddy Desktop, `./install --user`), the daemon's forked spawn worker
serves several requests at once. A ten-minute clone no longer holds every other session start on that
host behind it, a request's answer can never be delivered to a different caller, and one request can
report events as it goes (used by clone progress, node 5).

## Proposed Changes

### What's Changing
- Every worker request carries an id; responses and intermediate events are matched by id.
- The worker runs each request on its own thread; a slow request does not delay others.
- A worker that dies fails every pending request at once with a message naming the worker pid.

### What's Staying the Same
The worker is still forked before the async runtime starts; the requests it serves (spawn, clone) and
their results are unchanged.

## Acceptance Criteria
- [ ] A spawn requested while a clone is still running completes without waiting for the clone.
- [ ] Concurrent requests each receive their own result.
- [ ] A request can receive intermediate events before its result.
- [ ] Every pending request fails promptly, naming the worker, if the worker exits.

## Out of Scope
Cancelling a worker request (node 8); restarting a dead worker.
