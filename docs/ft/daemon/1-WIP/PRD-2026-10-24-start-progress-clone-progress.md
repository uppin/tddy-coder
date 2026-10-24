# A start that clones reports git's progress — PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement + Bug Fix
**Stack**: #start-progress 5/8 (base `feature/start-progress/split-stream`)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — provisioning a project at start
- **Related Feature**: [Cloning over SSH](../clone-ssh-passphrase.md)

## Summary

A session start that has to clone its project reports a **Cloning repository** step, and while it
runs, git's own progress: the stage (`Counting objects`, `Receiving objects`, `Resolving deltas`),
percent, object counts, bytes received and throughput. The clone has no deadline — it ends when git
does — and a clone that fails or is stopped leaves no half-written repository for the next start to
mistake for a complete one. This node covers the forked-worker and in-process clone; node 6 brings the
supervised clone to parity.

## Proposed Changes

### What's Changing
- `StartPhase` gains `STEP_CLONE`; `StartSessionEvent` gains a `CloneProgress` event.
- Clones run `git clone --progress` and stream git's stderr as parsed progress, throttled to a few
  events a second plus every stage change.
- A clone writes into a temporary sibling directory and renames it into place on success; on failure
  the temporary directory is removed.
- The clone deadline is removed from the session start and from CreateProject / AddProjectToHost.
- A clone has a handle that can stop it (used by Cancel, node 8).

### What's Staying the Same
Which projects clone and where; the SSH passphrase prompt; CreateProject / AddProjectToHost remain
unary and report no progress.

## Acceptance Criteria
- [ ] A start that clones emits `STEP_CLONE` begin, clone-progress events with stage, percent and
      bytes, then `STEP_CLONE` end before the worktree step.
- [ ] A start whose project is already present emits no clone step.
- [ ] A clone that outlives the old `spawn_worker_request_timeout_secs` completes.
- [ ] A failed clone leaves no destination directory, and its error carries git's message.
- [ ] A clone stopped through its handle ends its git process and leaves no destination directory.

## Out of Scope
Supervised clones (node 6); the UI (node 7); CreateProject / AddProjectToHost progress (TODO).
