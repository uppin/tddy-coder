# Supervised clones report progress and their own errors — PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: #start-progress 6/8 (base `feature/start-progress/clone-progress`)

## Affected Features

- **Primary Feature**: [tddy-supervisor](../../supervisor/tddy-supervisor.md) — brokered clones
- **Related Feature**: [Project concept](../project-concept.md)

## Summary

On a host run by `tddy-supervisor`, a clone a session start needs reports the same `Cloning
repository` progress as on any other host, and a failed clone's error carries git's own message
instead of only its exit code. A supervised clone can be stopped.

## Proposed Changes

### What's Changing
- The supervisor gains a streaming clone operation: it runs `git clone --progress` as the target user
  under the same spawn policy, streams git's progress lines, and ends with the exit status and the
  tail of git's error output.
- The daemon's supervised clone uses it, feeding the same progress events as node 5.
- A supervised clone is stoppable by the pid the stream reports.

### What's Staying the Same
The supervisor's authorization, user and environment policy; the existing spawn operations.

## Acceptance Criteria
- [ ] A supervised clone emits progress events, then succeeds.
- [ ] A failed supervised clone's error includes git's message.
- [ ] A supervised clone requested with an environment key outside `allowed_env_keys` is refused, as today.
- [ ] Stopping a supervised clone ends git and leaves no destination directory.

## Out of Scope
Streaming any other supervisor operation.
