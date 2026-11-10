# Effort on session start — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 3/11 (`feature/orchestrators/start-effort`)

## Affected Features

- **Primary**: [Project concept § StartSession](../project-concept.md#startsession) and
  [§ Start options](../project-concept.md#start-options) — `effort` is a start argument and an offered choice
- [Claude CLI session](../claude-cli-session.md) — how claude is launched at an effort
- [Session drawer § Create Session § Fields](../../web/session-drawer.md#fields) — an Effort picker

## Summary

A session can be started at an **effort level** — the reasoning depth its agent spends per turn — as
well as a model. The agent's YAML spec declares the levels it accepts and how the launch carries one;
claude's spec maps the level onto its launch. The New-session form shows an **Effort** picker for an
agent that declares levels, and the exec `StartSession` tool takes `effort`, so an orchestrator sizes a
session by model **and** effort, as `/yolo-pr-stack` sizes its subagents.

## Proposed Changes

### What's Changing
- `StartSessionRequest.effort` (string, field 48). Empty = the agent's default.
- Agent spec slot `effort: { levels: [...], default?, flag | env }`, validated at load; an agent with no
  slot refuses a non-empty effort with `INVALID_ARGUMENT` naming the agent.
- The level is recorded in `.session.yaml` and re-applied on resume.
- `DescribeSessionStartOptions` reports the levels per agent; `StartSession` offers `effort`.
- Web: Effort `<select>` beside Model, hidden for agents without levels.

### What's Staying the Same
- Model selection, harness profiles (a profile may later name an effort; not here).

## Acceptance Criteria
- [ ] A claude-cli session started at `high` launches with that level, and resumes with it
- [ ] An unknown level, or any level for an agent with no slot, is refused naming why
- [ ] The New-session form shows the agent's levels and sends the choice
- [ ] An orchestrator's `StartSession` with `effort` starts the session at it; `DescribeSessionStartOptions` lists the levels

## References
- [Project concept](../project-concept.md), [Claude CLI session](../claude-cli-session.md),
  [Session drawer](../../web/session-drawer.md)
