# Target permission answer - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md) — § "On a target session": the hooks that wait for an answer are installed on a target too.
- **Related Feature**: [Remote-managed worktree / session targets](../remote-managed-worktree.md) — the hook log and its follower.

## Summary

When an agent running on a target (a Coder workspace or an SSH host) asks for permission, the daemon records the prompt as the session's pending question, with its tool, its input and a `request_id`. The orchestrator can answer it with `AnswerSessionPermission`, as it answers a local session. The answer reaches the agent through a file in the session's worktree on the target, so the target still needs no tddy software.

## Background

On a target every hook is a `printf` that appends a line to `.tddy/hooks.log`, and the daemon follows that log through the session's remote shell. Nothing travels back, so the hooks that wait for an answer (`PermissionRequest`, and `PreToolUse` on `AskUserQuestion`) are not installed. Claude asks in its terminal. The daemon only sees the `Notification` that follows, which names no tool, so `ReadSessionActivity` shows `pending_question: null` and the prompt can only be answered by typing into the terminal (probe on `mantasi/remote-dev`, 2026-11-21).

## Proposed Changes

### What's Changing
- A target session's settings install `PermissionRequest` as a **waiting** hook. It is still POSIX-only: it appends its line, carrying a nonce of its own, to `.tddy/hooks.log`, then waits for `.tddy/answers/<nonce>` within the session's answer wait and prints that file as Claude's decision.
- The daemon's follower reports the line as a local hook would: pending question, notification and `request_id`. It holds for the orchestrator's answer as `AwaitQuestionAnswer` does, then writes Claude's decision into `.tddy/answers/<nonce>` through the session's shell.
- With no orchestrator, or no answer within the wait, no decision is written. The hook gives up and Claude asks in its terminal, as a local session does.
- The answers directory is removed with the hook log when the session ends.

### What's Staying the Same
- Local sessions' hooks and their answer flow.
- `AnswerSessionPermission`, `ReadSessionActivity` and the notification shapes.
- No tddy software on the target.

## Impact Analysis

### Technical Impact
tddy-agent-backend (the waiting remote hook command), tddy-agent-launch (follower: nonce, hold, answer write), tddy-session-activity (the decision JSON moves here from tddy-tools; the delivery port returns the `request_id`), tddy-tools (uses the moved functions), tddy-session-lifecycle (delivery wiring).

### User Impact
An orchestrator sees and answers the permission prompts of its remote children. On Coder each answer costs one shell round trip (~6–9 s) before the agent continues.

## Implementation Plan

One node: `feature/target-parity/permission-answer` (#target-parity 1/7).

## Acceptance Criteria

- [ ] A permission prompt of an agent on a target becomes the session's pending question, with its tool, its input and a `request_id`.
- [ ] `AnswerSessionPermission` on it lets the agent proceed (allow) or refuses the call (deny), as for a local session.
- [ ] With no orchestrator present, or no answer within the session's wait, the agent's own terminal prompt appears.
- [ ] Nothing of tddy is needed on the target; the answers directory is gone when the session ends.
