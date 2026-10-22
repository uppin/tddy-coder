# Project-home session auto-stop on idle - PRD

**Date**: 2026-10-22
**PRD Type**: Enhancement
**Stack**: #project-home 6/7 (`feature/project-home/home-auto-stop`, base `feature/project-home/home-session-tools`)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — the project-home session's lifetime.
- **Related Feature**: [Claude CLI session](../claude-cli-session.md) — stop and resume keep the agent conversation.
- **Related Feature**: [Agent session status](../agent-session-status.md) — idleness is read from it.

## Summary

A running project-home session is **stopped automatically** once its agent has been idle — its last
turn ended and no new input arrived — for `project_home.idle_stop_secs` (daemon config, default 600).
The session keeps its agent conversation id, so the next click on its pinned entry resumes the same
conversation (node 4's ensure). No other session is ever auto-stopped.

## Background

The home session is pinned and one click away, so it will be opened casually and left. Without a stop
it holds an agent process (and a provider's context) per project indefinitely.

## Proposed Changes

### What's Changing
- A daemon task checks running project-home sessions. A session is **idle** while its agent status is
  `idle` or `waiting_for_input`; the idle clock starts at the later of its last turn end and its last
  input. A session `running`, `executing_tool` or `connecting` is never stopped.
- When idle for at least `idle_stop_secs`, the session is stopped through the normal stop path; its
  status reads stopped and its `.session.yaml` keeps what resume needs.
- `idle_stop_secs: 0` disables auto-stop.

### What's Staying the Same
- Manual stop/resume; ensure's started/resumed/running decision; every non-home session.

## Impact Analysis

### Technical Impact
- Daemon config section; an idle policy (pure) and a periodic task in the daemon runtime.

### User Impact
- Coming back to a home session after a while costs a resume. The conversation is kept.

## Implementation Plan
1. Config section with default.
2. Pure idle policy.
3. Periodic task over the session listing, stopping via the existing path.

## Acceptance Criteria
- [ ] An idle project-home session is stopped after `idle_stop_secs`.
- [ ] A project-home session mid-turn is not stopped.
- [ ] New input resets the idle clock.
- [ ] After an auto-stop, ensuring the home session resumes the same conversation.
- [ ] Ordinary sessions are never auto-stopped; `idle_stop_secs: 0` disables it.

## References

### Affected Features (Complete List)
- [project-concept.md](../project-concept.md), [claude-cli-session.md](../claude-cli-session.md), [agent-session-status.md](../agent-session-status.md)

### Related Documentation
- Predecessor: `feature/project-home/home-session` (4/7) owns ensure and resume.
