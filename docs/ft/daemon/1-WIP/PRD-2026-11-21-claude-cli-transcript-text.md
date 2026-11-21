# Claude CLI transcript text - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md) — the transcript of a claude-cli session.
- **Related Feature**: [Agent activity pane](../../web/agent-activity-pane.md) — the Activities view replays the same frames.

## Summary

A claude-cli session's transcript (`ReadSessionTranscript`, the web's Activities view) holds what was said, not only the tool calls. Each prompt the agent was given is a user message, and each reply it finished a turn with is an agent message, in order among the tool calls. This holds for local and target sessions alike.

## Background

A claude-cli session writes no `acp-transcript.jsonl`; only the coder presenter does. Its transcript is therefore its tool calls alone, and an orchestrator following a child cannot read what the child said. Both texts already reach the daemon in hooks the session installs: `UserPromptSubmit` carries `prompt`, and `Stop` carries `last_assistant_message`. Recorded as [2026-10-25-claude-cli-transcript-has-no-conversation-text.md](../../../dev/todo/2026-10-25-claude-cli-transcript-has-no-conversation-text.md).

## Proposed Changes

### What's Changing
- `UserPromptSubmit` appends the prompt to the session's transcript as a user message, and `Stop` appends the last assistant message as an agent message, both timestamped where they happened.

### What's Staying the Same
- Tool-call rows, the transcript readers and their paging, and tool sessions' transcripts.

## Impact Analysis

### Technical Impact
tddy-session-activity (the status-report handler writes the two frames).

### User Impact
An orchestrator sees each child's prompt and reply; the Activities view reads as a conversation.

## Implementation Plan

One node: `feature/target-parity/transcript-text` (#target-parity 3/7).

## Acceptance Criteria

- [ ] After a prompt and a reply, `ReadSessionTranscript` lists the prompt as a user message and the reply as an agent message, in order.
- [ ] Tool calls keep their place between them.
- [ ] A turn that ends with no reply text adds no empty message.
