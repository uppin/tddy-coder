# The create pane shows each step and a live clone bar — PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: #start-progress 7/8 (base `feature/start-progress/supervised-clone-progress`)

## Affected Features

- **Primary Feature**: [Session drawer](../session-drawer.md) § Start progress — the create pane

## Summary

While a session is being created, the create pane lists the steps of the start under a heading naming
the host doing the work, each marked pending, in progress or done. A start that clones shows a
**Cloning repository** row with git's current stage, a percent bar, bytes received and throughput.

```
Creating session on mantas-udoo-ros

  ✓ Connected to codebase host
  ◐ Cloning repository
      Receiving objects  45%  ███████░░░░░  38.2 MiB · 4.1 MiB/s
  ○ Worktree
  ○ Agent
```

## Proposed Changes

### What's Changing
- The single "Creating worktree…" line becomes a step list: Cloning repository (only when the host
  reports one), Worktree, Semantic index (only when requested), Agent.
- The clone row shows stage, percent bar, bytes and throughput, updated as events arrive.
- A step that the host reports ending is ticked; the step in progress shows a spinner.
- A failed start leaves the list showing where it stopped, with the error under it.

### What's Staying the Same
Attachment progress rows; the passphrase prompt; Create disabled while a start runs.

## Acceptance Criteria
- [ ] A streamed start's steps appear in order, ticked as each ends.
- [ ] Clone progress shows stage, percent, bytes and throughput, and updates in place.
- [ ] A start that does not clone shows no clone row.
- [ ] A failed start shows the step it stopped at and the error.

## Out of Scope
The Cancel action (node 8).
