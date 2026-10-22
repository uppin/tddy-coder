# Project home agent setting - PRD

**Date**: 2026-10-22
**PRD Type**: Enhancement
**Stack**: #project-home 1/7 (`feature/project-home/home-agent-setting`, base `master`)

## Affected Features

- **Primary Feature**: [Projects screen (multi-host)](../projects-screen-multi-host.md) — each project row gains a **Home agent** picker.
- **Related Feature**: [Project concept](../../daemon/project-concept.md) — a project record gains an optional `home_agent`.
- **Related Feature**: [Session agent catalog fan-out](../session-agent-catalog-fan-out.md) — the picker lists what `CatalogService.ListAgents` lists, harness profiles included.

## Summary

A project can name the agent its **project-home session** runs. The Projects screen shows a
**Home agent** picker per project, filled from the agent catalog; the choice is stored on the project
record and reported by `ListProjects`. Unset is a valid state that means "no home agent configured".

## Background

The `#project-home` stack pins one managed, read-only session per project at the top of that
project's session group (node 4, `feature/project-home/home-session`). It starts on click without a
create dialog, so the agent it runs must already be decided — per project, because projects use
different harnesses and provider accounts. This node is that decision and nothing else.

## Proposed Changes

### What's Changing
- `ProjectEntry` gains `home_agent` (string; empty = unset).
- New RPC `ProjectService.SetProjectHomeAgent(project_id, home_agent, daemon_instance_id)` returning the
  updated `ProjectEntry`. An empty `home_agent` clears the setting. A non-empty id that
  `ListAgents` does not list is refused (`invalid_argument`) and nothing is written.
- Projects screen: a **Home agent** `<select>` per project row with `Not set` plus one option per
  catalog agent (label from `AgentInfo.label`). Changing it calls `SetProjectHomeAgent` and shows the
  stored value afterwards. A refusal is shown inline on the row.

### What's Staying the Same
- Every other project field and setter (`SetProjectDefaultBranch`, `SetProjectAccounts`).
- Session start: nothing reads `home_agent` in this node.
- The write reaches one host, exactly as the existing setters do (see Impact).

## Impact Analysis

### Technical Impact
- `tddy-service` proto (`project.proto`), `tddy-projects` (storage + service + handler), `tddy-daemon-rpc`
  (coordinate handler), `tddy-web` (Projects screen).
- `projects.yaml` gains an optional key; rows written by older daemons read as unset.

### User Impact
- One new control per project row. No migration.

## Implementation Plan
1. Proto field + RPC.
2. `ProjectData.home_agent`, `set_project_home_agent` with catalog validation.
3. Daemon-rpc coordinate handler routed like `SetProjectDefaultBranch`.
4. Projects screen picker.

## Acceptance Criteria
- [ ] `SetProjectHomeAgent` with a listed agent id persists it and `ListProjects` reports it.
- [ ] `SetProjectHomeAgent` with an empty id clears it.
- [ ] `SetProjectHomeAgent` with an id the catalog does not list is refused and leaves the stored value unchanged.
- [ ] The Projects screen shows the stored home agent per project and changes it through the RPC.
- [ ] A refused change shows the reason on that project's row.

## References

### Affected Features (Complete List)
- [projects-screen-multi-host.md](../projects-screen-multi-host.md) — Home agent picker
- [project-concept.md](../../daemon/project-concept.md) — `home_agent` field

### Related Documentation
- Successor PRs: `feature/project-home/home-session` (#project-home 4/7) consumes `home_agent`.
