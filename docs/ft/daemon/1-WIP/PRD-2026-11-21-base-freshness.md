# An orchestrator checks and refreshes its base, and a start reports the commit it was cut from - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 4/9 — branch `feature/orchestra/base-freshness`, base `feature/orchestra/session-files`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

The orchestrator runs the freshness checks itself: `CheckBaseFreshness` fetches the project's trunk and reports its home worktree's commit, the remote tip, and ahead/behind; `RefreshHome` fast-forwards the home to the tip. Every start the orchestrator makes records and reports the base commit it was cut from and how far that commit was behind the remote tip at cut time (`GetSessionStart`).

## Background

A home worktree is fast-forwarded only at start and resume (`orchestrator_start.rs:87-117`); after that the orchestrator plans from whatever it had, and nothing reports which base a child was cut from.

## Proposed Changes

### What's Changing
- `CheckBaseFreshness` → `{ trunk_ref, home_commit, remote_tip, ahead, behind, fetched_at }`.
- `RefreshHome` → fast-forward only; refuses (and says why) when the home has diverged.
- `GetSessionStart` / the start ledger entry carry `base_ref`, `base_commit`, `base_behind`.

### What's Staying the Same
- Automatic fast-forward at start/resume.
- How a new session's base is chosen.

## Impact Analysis

### Technical Impact
- **tddy-worktree-service**: `base_freshness` (fetch + ahead/behind of the home worktree vs the project's trunk tip) and `refresh_home` (`fast_forward_home_worktree`)
- **tddy-tool-engine**: `CheckBaseFreshness`, `RefreshHome` tools; start ledger records `base_commit` and `base_behind`
- **tddy-daemon-rpc**: role guard + serving

### User Impact
An orchestrator knows whether it is planning against today's trunk, and every child's base is on record.

## Implementation Plan

One node: `feature/orchestra/base-freshness` (#orchestra 4/9).

## Acceptance Criteria

- [ ] `CheckBaseFreshness` reports behind > 0 after a commit lands on the remote trunk.
- [ ] `RefreshHome` brings behind to 0.
- [ ] `RefreshHome` refuses a diverged home without changing it.
- [ ] A started session's `GetSessionStart` names its base commit and behind count.
