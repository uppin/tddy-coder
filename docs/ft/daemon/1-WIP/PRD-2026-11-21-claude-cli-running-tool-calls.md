# Claude CLI running tool calls - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md) — hook wiring.
- **Related Feature**: [Agent activity pane](../../web/agent-activity-pane.md) — running calls.

## Summary

A claude-cli session's tool call shows as running from the moment the agent starts it, with its start time, and is completed by its result. This holds for local and target sessions.

## Background

`PreToolUse` is installed only for `AskUserQuestion` (`#home-inbox` 3/8), and on a target not at all. Every activity row is therefore written by `PostToolUse` with `started_unix_ms: 0`, and no call is ever seen running. Long `Bash` runs look idle.

## Proposed Changes

### What's Changing
- Every claude-cli session installs a reporting `PreToolUse` hook for every tool. Its report writes the `running` row, and the `PostToolUse` of the same call (paired by Claude's `tool_use_id`) completes it.
- The question tools keep their own waiting hook. The catch-all's report of a question tool records no question and no tool call.

### What's Staying the Same
- Questions and permission prompts; the activity row format.

## Impact Analysis

### Technical Impact
tddy-agent-backend (the settings builder), tddy-session-activity (the catch-all report of a question tool is not a question).

### User Impact
Running calls and their durations are visible. Locally, one short `tddy-tools` process runs per tool call; on a target it is one `printf`.

## Implementation Plan

One node: `feature/target-parity/running-tool-calls` (#target-parity 4/7).

## Acceptance Criteria

- [ ] A tool call is recorded running, with its start time, before its result arrives — locally and on a target.
- [ ] Its result completes the same call (one call, not two).
- [ ] An `AskUserQuestion` is still recorded once, as a question, and never as a tool call.
