# An orchestrator's child is told how to report back, and a target child's report reaches it - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 2/9 — branch `feature/orchestra/child-back-channel`, base `feature/orchestra/path-reach`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

Every session started by an orchestrator gets launch guidance that names its orchestrator and **the one back channel it should use to report progress and status**: a Claude child on the orchestrator's host messages it natively as a Claude peer; any other local child runs `tddy-tools send`; a child on a Coder or SSH target appends a `message` line to its worktree's `.tddy/hooks.log`, which the daemon already tails and now turns into a session-message record for the orchestrator.

## Background

A child today learns nothing about its orchestrator (`agent_notify_guidance.md` covers only `notify` and `rename-session`). `PostSessionMessage` exists but only for local agents that can run `tddy-tools`; a target has no `tddy-tools`, and `HookLine::parse` drops any event it does not know.

## Proposed Changes

### What's Changing
- Children of an orchestrator get a guidance file naming the orchestrator's session id, its native peer name where both are Claude on one host, and the exact command to report with.
- Claude children are launched with a pinned peer name so the orchestrator can address them natively; the name is recorded with the session and listed by `ListProjectSessions`.
- `HookLine::parse` accepts a `message` event; the follower writes it as a `message` record addressed to the session's orchestrator, reaching its inbox watch exactly as `PostSessionMessage` does.

### What's Staying the Same
- `PostSessionMessage` / inbox / watches semantics.
- Sessions without an orchestrator get no back-channel guidance.

## Impact Analysis

### Technical Impact
- **tddy-agent-launch**: child guidance (orchestrator id, back channel by placement); `message` line handled by the remote hook follower, in a new module
- **tddy-core**: `remote_message_command` — the `printf` line a target child appends to `.tddy/hooks.log`
- **tddy-session-activity**: a target child's message lands as the same `message` record `PostSessionMessage` writes
- **tddy-coding-agent-specs**: how an agent is addressed natively (peer name), declared per agent

### User Impact
Children report progress on their own; reports from Coder/SSH children arrive the same way as local ones.

## Implementation Plan

One node: `feature/orchestra/child-back-channel` (#orchestra 2/9).

## Acceptance Criteria

- [ ] A local non-Claude child's guidance names its orchestrator and `tddy-tools send`.
- [ ] A Claude child on the orchestrator's host is told its orchestrator's native peer name, and is itself launched under a pinned one.
- [ ] A target child's guidance gives the `printf` line, and that line reaches the orchestrator as a `message` record.
- [ ] A `message` line from a session with no orchestrator is dropped, with a log line.
