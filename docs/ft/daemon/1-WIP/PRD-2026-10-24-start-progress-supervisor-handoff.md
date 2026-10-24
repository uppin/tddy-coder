# A supervised daemon knows it is supervised — PRD

**Date**: 2026-10-24
**PRD Type**: Bug Fix + Configuration change
**Stack**: #start-progress 3/8 (base `feature/start-progress/worker-mux`)

## Affected Features

- **Primary Feature**: [systemd install](../systemd-install.md) — `daemon.yaml`'s `supervisor:` block
- **Related Feature**: [tddy-supervisor](../../supervisor/tddy-supervisor.md) — managed services

## Summary

A daemon started by `tddy-supervisor` spawns sessions and clones through it, because the supervisor
tells the daemon where its socket is when it starts it. There is no `supervisor:` block in
`daemon.yaml` any more, so a host can no longer be supervised in fact and unsupervised in config — the
state that broke cloning on `mantas-udoo-ros`.

## Proposed Changes

### What's Changing
- `tddy-supervisor` gives every managed service it starts the path of its privileged socket in the
  environment. `supervisor.yaml`'s per-service `env:` cannot set or override it.
- The daemon chooses the supervisor backend when, and only when, it was started that way. Started any
  other way (desktop, `--user`, by hand), it forks its own spawn worker as today.
- `daemon.yaml` loses the `supervisor:` key. A file that still has it fails to load with a message
  naming the block as obsolete and saying to delete it.
- `./install --systemd` and `--user` delete the block from an existing `daemon.yaml` on upgrade, and
  say so.

### What's Staying the Same
The supervisor socket's path, group and mode; that an unreachable supervisor fails a spawn closed
(no fallback to the worker); `spawn_policy`.

## Acceptance Criteria
- [ ] A daemon started by the supervisor clones and spawns through it with no `supervisor:` block.
- [ ] A daemon started without the supervisor forks its own worker.
- [ ] A `supervisor:` value in a service's `env:` does not reach the daemon.
- [ ] A `daemon.yaml` with `supervisor:` fails to load, naming the block and the remedy.
- [ ] `./install` on a host whose `daemon.yaml` has the block removes it and reports that it did.

## Out of Scope
Changing the supervisor's spawn policy; the desktop app (never supervised).
