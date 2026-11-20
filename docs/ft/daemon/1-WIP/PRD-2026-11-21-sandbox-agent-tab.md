# Sandbox agent tab - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Session liveness](../session-liveness.md) — `[sandbox, agent]` chains; a jailed agent tab is a live agent tab.
- **Related Feature**: [Claude CLI session — sandboxed](../claude-cli-session.md) and [Terminal sessions](../terminal-sessions.md) — agent tabs in jails of their own.
- **Related Feature**: [Session terminal tabs](../../web/session-terminal-tabs.md) — agents listed under "In sandbox".

## Summary

An agent tab opened "In sandbox" on a sandboxed session runs the agent in its **own jail** with the session's worktree and the agent's sandbox grants, in a conversation of its own. It keeps the session live like any agent tab, is recorded as `sandbox` → `agent`, and Resume restores it into a new jail, resuming its conversation where the harness can.

## Background

n5 gives a sandboxed session jailed shell tabs. An agent tab needs more: an exec plan built from the agent's spec and grants, a conversation, and a home that may be the same persistent home the main jail is using.

## Proposed Changes

### What's Changing
- `sandbox` around an agent is accepted; the tab-jail launch builds the agent's exec plan (as the main's jail does) under the tab's own jail paths, with a fresh conversation.
- A persistent agent home shared with the main jail is used without clobbering the main's session records (role, usage context).
- The tab counts as a live agent tab; it is stopped with the session.
- Recorded as `[sandbox, agent]` (harness, conversation, resumable); Resume restores it into a new jail and resumes its conversation where the harness can, otherwise reopens fresh and says so.
- Web: agent kinds appear under "In sandbox".

### What's Staying the Same
- n5's jail registry, paths, teardown and menu; the main jail.

## Impact Analysis

### Technical Impact
tddy-agent-launch (tab-jail agent exec plan, restore), tddy-daemon-sandbox (agent grants per tab), tddy-session-lifecycle (liveness counting), tddy-session-web.

### User Impact
Extra agents of a sandboxed session stay confined and come back on Resume.

## Implementation Plan

One node: `feature/session-liveness/sandbox-agent-tab` (#session-liveness 7/8).

## Acceptance Criteria

- [ ] An agent tab opened "In sandbox" runs in a jail of its own, in its own conversation.
- [ ] It keeps the session live; stopping the session stops its jail.
- [ ] The main's session records are not overwritten by the tab's launch.
- [ ] Recorded as `sandbox` → `agent`; Resume restores it into a new jail, resuming its conversation where the harness can.
