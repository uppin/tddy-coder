# 2026-07-24 — activities-as-acp: `AgentActivityOverlay` body becomes `<AgentChatView readOnly>` fed by new `useAcpReplay` (decodes `StreamAcpReplay` `AcpReplayFrame` bytes → `AcpAgentMessage`, coalesces tool frames by `tool_call_id`). `AgentChatView` gains a `readOnly` mode (tool-call cards + `+Ns` timing badge from `timestamp_unix_ms`); the streaming agent-chunk merge is extracted to shared `acpAgentMerge.ts`; `ChatMessage.from` gains `"tool"` + optional `toolStatus`. Legacy `AgentActivityAcceptance.cy.tsx` removed (row-list UI gone); Cypress `AgentActivityAcpReplayAcceptance` 8. Feature [agent-activity-pane.md](../../../../docs/ft/web/agent-activity-pane.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)

**Type:** Feature


