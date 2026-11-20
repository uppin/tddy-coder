# Target agent tab - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Session liveness](../session-liveness.md) — `[target, agent]` chains.
- **Related Feature**: [Remote-managed worktree / session targets](../remote-managed-worktree.md) — agent tabs run on the target.
- **Related Feature**: [Session terminal tabs](../../web/session-terminal-tabs.md) — agents listed under "On target".

## Summary

An agent tab opened "On target" runs the agent on the target, in the session's worktree there, resolved by the target's own login `PATH` and in a conversation minted on the target. Recorded as `target` → `agent`; Resume reopens it on the target and resumes its conversation where the harness can.

## Background

n6 gives target sessions shell tabs on the target. Today a new agent tab resolves the agent binary on the daemon host and mints its chat there (`new_agent_tab`), so a daemon host without the agent refuses it, and a target agent tab would start a conversation in the wrong place.

## Proposed Changes

### What's Changing
- `target` around an agent is accepted; the agent's argv names the bare command (resolved on the target) and its chat is minted on the target.
- Launched through n6's non-main target client launch, with the agent's rendered environment carried in the command.
- Recorded as `[target, agent]`; Resume reopens it on the target, resuming its conversation where the harness can.
- The tab half of "a target start requires the agent on the daemon host" is resolved.

### What's Staying the Same
- The target main's launch; n6's shell tabs.

## Impact Analysis

### Technical Impact
tddy-agent-launch (agent tab on a target: binary and chat resolution, restore), tddy-session-lifecycle, tddy-session-web.

### User Impact
Extra agents of a remote session run where the code is, even when the daemon host has no agent installed.

## Implementation Plan

One node: `feature/session-liveness/target-agent-tab` (#session-liveness 8/8).

## Acceptance Criteria

- [ ] An agent tab opened "On target" runs on the target in the session's worktree.
- [ ] It starts even when the daemon host has no copy of the agent.
- [ ] Its conversation is minted on the target.
- [ ] Recorded as `target` → `agent`; Resume reopens it on the target, resuming its conversation where the harness can.
