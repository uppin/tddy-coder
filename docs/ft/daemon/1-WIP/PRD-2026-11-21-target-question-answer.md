# Target question answer - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md) — § "On a target session".

## Summary

When an agent on a target asks a question (`AskUserQuestion`), the question becomes the session's pending question, and the orchestrator's `AnswerSessionQuestion` reaches the agent through the answer channel that permission prompts use.

## Background

After #target-parity 1/7 a target answers permission prompts through `.tddy/answers/<nonce>`. The `PreToolUse` hook matched to `AskUserQuestion` is still not installed on a target, so a remote agent's question appears only in its terminal.

## Proposed Changes

### What's Changing
- A target session installs the waiting `PreToolUse` hook on `AskUserQuestion`. The daemon reports the question, holds for the answer and writes Claude's question decision (the chosen answers) through the same channel.

### What's Staying the Same
- The channel, permission prompts and local sessions.

## Impact Analysis

### Technical Impact
tddy-agent-backend, tddy-agent-launch (question decisions on the channel).

### User Impact
Remote children's questions are answered by the orchestrator like local ones.

## Implementation Plan

One node: `feature/target-parity/question-answer` (#target-parity 6/7).

## Acceptance Criteria

- [ ] A question asked by an agent on a target is the session's pending question.
- [ ] `AnswerSessionQuestion` delivers the chosen answers to the agent.
- [ ] Unanswered, the agent's own question dialog appears.
