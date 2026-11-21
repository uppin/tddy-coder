# An orchestrator asks the operator for read or write access to named paths - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 6/9 — branch `feature/orchestra/access-elicitation`, base `feature/orchestra/trunk-watch`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

An orchestrator that needs a path outside its reach names the paths itself and calls `RequestPathAccess` with a mode (`read` or `write`) and a reason. The request appears in the orchestrator's session pane with Allow and Deny, and an attention notification goes out (drawer, desktop, Telegram). Allow writes a session grant (n1's store); Deny answers the tool with the refusal. Write grants let the orchestrator's `Write`, `StrReplace` and `Delete` act inside the granted paths only.

## Background

An orchestrator is read-only and confined; there is no way for it to ask for more, and the operator has no place to answer.

## Proposed Changes

### What's Changing
- `RequestPathAccess` creates a pending access request and waits for the answer (bounded; an unanswered request times out as denied).
- The bus carries `PATH_ACCESS_REQUESTED` like other attention kinds.
- The orchestrator's session pane shows pending requests with Allow / Deny; `AnswerPathAccess` resolves them.
- Write tools for orchestrator roles: refused unless every touched path is inside a write grant.

### What's Staying the Same
- Default reach (n1).
- Answers to child sessions' permissions (`AnswerSessionPermission`).

## Impact Analysis

### Technical Impact
- **tddy-tool-engine**: `RequestPathAccess { paths, mode, reason }` (blocks until answered or times out); write tools admit granted paths
- **tddy-session-activity**: pending access request record, answer, grant written to n1's `PathGrantStore`
- **tddy-service**: `activity.proto`: `AnswerPathAccess` RPC, `PATH_ACCESS_REQUESTED` notification kind
- **tddy-daemon-rpc**: role guard: Write/StrReplace/Delete admitted inside write grants only
- **tddy-session-web**: the request in the orchestrator's session pane: paths, mode, reason, Allow / Deny

### User Impact
The operator decides, path by path, what an orchestrator may read or change beyond its defaults, and is notified when it asks.

## Implementation Plan

One node: `feature/orchestra/access-elicitation` (#orchestra 6/9).

## Acceptance Criteria

- [ ] A request shows in the orchestrator's pane and raises an attention notification.
- [ ] Allow makes the read succeed; the grant is listed in the session's grants.
- [ ] Deny returns a refusal to the agent.
- [ ] An unanswered request times out as denied.
- [ ] A write inside a write grant succeeds; outside it is refused.
