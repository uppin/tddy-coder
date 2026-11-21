# A target session starts without the agent on the daemon host - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Remote-managed worktree / session targets](../remote-managed-worktree.md).
- **Related Feature**: [Claude CLI session](../claude-cli-session.md).

## Summary

Starting a session on a target (ssh or Coder) resolves the agent on the target, not on the daemon host: the spec gives the bare command, the target's login `PATH` finds it, its chat is minted on the target, and a probe checks it exists before the worktree is cut — failing fast with "not installed on the target" otherwise.

## Background

A target start still resolves the agent binary on the daemon host first and refuses when the host has none, although the agent runs on the target (the tab half was fixed in `#session-liveness` 8/8).

## Proposed Changes

### What's Changing
- A target start looks the spec up without a host binary (as target agent tabs do) and probes the bare command on the target with `probe_agent_on_target` (#tab-hardening 1/4) before cutting the worktree.
- Resume of a target session does the same.

### What's Staying the Same
- Host and sandboxed starts; tools the daemon itself runs (e.g. `tddy-tools`) stay host-resolved.

## Acceptance Criteria

- [ ] A target session starts when the daemon host has no copy of the agent.
- [ ] A target whose agent is missing fails the start before any worktree is cut, naming the command.
- [ ] A target session resumes without the agent on the daemon host.
