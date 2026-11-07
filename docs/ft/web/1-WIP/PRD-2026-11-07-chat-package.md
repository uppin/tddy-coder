# Agent chat package (`tddy-chat-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 10/16 — branch `feature/web-split/chat`

## Affected Features

- **Primary**: [agent-activity-pane.md](../agent-activity-pane.md), [enqueued-input-overlay.md](../enqueued-input-overlay.md), [inactive-session-activities.md](../inactive-session-activities.md) — agent chat, ACP transcript, tool-call detail, activity panes.
- **Related**: PR-stack chat and models (consume the chat).

## Summary

Move agent chat — `AgentChat`, `LiveAgentChatView`, `AgentTranscriptView`, the ACP session/replay/tool-call hooks, transcript and presentation helpers, the agent activity registry, activity panes and the subagent conversation/permission UI — into `packages/tddy-chat-web`, with its ~21 specs.

## Background

Chat is consumed by sessions, PR-stack chat and the models screen. Its LiveKit reach (`useAgentChat`, `useAcpSession`, `AgentChat`) is behind the ports since `feature/web-split/livekit`, and moving `agentActivityRegistry` with it breaks the chat → sessions cycle.

## Proposed Changes

### What's Changing

- New package exporting the chat components and hooks; `agentActivityRegistry` and the activity/subagent panes move in; its PR check.

### What's Staying the Same

- Chat, ACP streaming, elicitation, export, activity and permission behaviour.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] Chat lives in `tddy-chat-web` with no `tddy-web`/LiveKit import.
- [ ] Its specs pass from its own PR check.
- [ ] Sessions, PR-stack and models render chat unchanged.

## References

- Predecessors (real): `livekit` (chat's room use behind ports), `clients`, `ui`. Successors: `workspace` (models), `prstack`, `session`.
