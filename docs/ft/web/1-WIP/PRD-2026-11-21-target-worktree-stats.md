# Target worktree stats - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Session worktree inspector](../session-worktree-inspector.md) — the Worktree tab of a target session.
- **Related Feature**: [Worktree disk usage streaming](../worktree-disk-usage-streaming.md).

## Summary

The Worktree tab of a session whose agent runs on a target shows that session's worktree on the target: branch, diff summary and disk size, measured there. It no longer shows the worktree as missing.

## Background

`StreamWorktreeStats` lists only the worktrees of the project's clone on the daemon host. A target session's worktree is never among them, so the tab finds no row and offers Restore.

## Proposed Changes

### What's Changing
- The project's worktree stream includes the worktrees of the project's target sessions, as rows keyed by their target path. Branch and diff are read with git on the target, and size with `du`, through each session's shell.
- They are measured as local rows are: sized lazily, and calculating until the target answers, so a slow transport never delays the local rows.
- An unreachable target leaves its row marked stale, with no numbers.

### What's Staying the Same
- Local rows and the proto. The web needs no change beyond rendering the row it now finds.

## Impact Analysis

### Technical Impact
tddy-worktree-service (target rows in the snapshot and the size calculator, through the target-shell port from #target-parity 2/7), tddy-workspace-web (nothing, or a "on target" hint).

### User Impact
Remote sessions' worktrees are visible, with their size and changes.

## Implementation Plan

One node: `feature/target-parity/target-worktree-stats` (#target-parity 7/7).

## Acceptance Criteria

- [ ] The tab of a target session shows its worktree on the target, with branch, diff summary and size.
- [ ] Local rows are listed without waiting for the target.
- [ ] An unreachable target marks the row stale rather than missing.
