# An orchestrator reaches each child by its best route and releases a terminal it claimed - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 7/9 — branch `feature/orchestra/orchestrator-delivery`, base `feature/orchestra/access-elicitation`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

For each child, the daemon tells the orchestrator how to reach it: `native` (a Claude child on the same host, by the peer name n2 records), `inbox` (`PostSessionMessage` + wake, for local agents with tddy-tools), or `terminal` (`SendSessionMessage` typed into its TUI — preferred for Coder/SSH children). Cursor and Muse declare `turn_input: terminal_paste`, so `SendSessionMessage` reaches them too. An orchestrator can release a terminal it claimed (`ReleaseSessionTerminal`), and a claim is forgotten when its session ends.

## Background

Only Claude declares `turn_input`; there is no release for `ClaimSessionTerminal` ("there is no release", `project_home_terminal_tools.rs:95`); the lease map only grows.

## Proposed Changes

### What's Changing
- `ListProjectSessions` rows gain `delivery_route` and, for native, the peer name.
- `SendSessionMessage` to a Coder/SSH child goes through its terminal even where an inbox exists.
- Cursor and Muse YAML declare `turn_input`.
- `ReleaseTerminalControl { screen_id, control_token }` releases a lease held by that token; `ReleaseSessionTerminal` calls it for the caller's own claim only.
- Session end drops its leases and the caller-side record.

### What's Staying the Same
- Claim semantics (steal).
- ACP turn input (still refused; its todo stays open).

## Impact Analysis

### Technical Impact
- **tddy-daemon-rpc**: `ListProjectSessions` reports each child's `delivery_route`; `ReleaseSessionTerminal`; `HomeTerminalLeases::forget`
- **tddy-coding-agent-specs**: `turn_input: terminal_paste` for Cursor and Muse
- **tddy-terminal-rpc**: `ReleaseTerminalControl` RPC
- **tddy-cli-sessions**: `control_lease` release; leases dropped when the session ends
- **tddy-tool-engine**: `ReleaseSessionTerminal` tool

### User Impact
Messages reach every agent kind, remote children are driven through their TUI, and the operator gets a terminal back without stealing it.

## Implementation Plan

One node: `feature/orchestra/orchestrator-delivery` (#orchestra 7/9).

## Acceptance Criteria

- [ ] A Claude child on the orchestrator's host is listed with route `native` and its peer name.
- [ ] A Coder child is listed with route `terminal`, and a message is typed into its TUI.
- [ ] A message reaches a Cursor child and a Muse child by terminal paste.
- [ ] After `ReleaseSessionTerminal`, the web screen can take control without stealing.
- [ ] Releasing a claim the caller does not hold is refused.
- [ ] A session's leases are gone after it ends.
