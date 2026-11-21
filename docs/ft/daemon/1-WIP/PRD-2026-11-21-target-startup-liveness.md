# Target startup liveness - PRD

**Date**: 2026-11-21
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md) — starting an agent on a target.

## Summary

The check that an agent on a target is still alive after it started gets enough time for one round trip of the target's transport. An agent that dies during startup on a Coder workspace fails the start, with its last output.

## Background

After the agent records its pid, the start probes it with `kill -0` for the startup window (500 ms by default), and each probe is cut off at what the window has left. A Coder round trip takes 6–9 s, so the probe never answers, and a probe that does not answer counts as alive (`timed_out` after 504 ms on `mantasi/remote-dev`, 2026-11-21). An agent that died is noticed only when its terminal ends. #963 bounded only the wait for the pid.

## Proposed Changes

### What's Changing
- The liveness probe is bounded by the start's transport allowance, not by what is left of the window, and the agent is judged alive only by a probe that answered.

### What's Staying the Same
- The startup window, the pid wait (#963) and local starts.

## Impact Analysis

### Technical Impact
tddy-agent-launch (`remote_agent_launch/startup.rs`).

### User Impact
A remote agent that fails at once is reported as a failed start, not as a running session.

## Implementation Plan

One node: `feature/target-parity/startup-liveness` (#target-parity 5/7).

## Acceptance Criteria

- [ ] On a target slower than the startup window, an agent that exits right after recording its pid fails the start.
- [ ] An agent that stays up on such a target still starts.
