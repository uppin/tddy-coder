# 2026-09-09 — Two generated clients, and 78 exports leave `connection_pb.ts`

`session_agents_pb.ts` and `activity_pb.ts` are generated; `connection_pb.ts` loses 78 exports and
`types_pb.ts` gains `SessionAgentStatus` and `SessionAgentActivity`, which `SessionEntry` now reaches
there.

25 call sites are re-pointed at the two new clients, following node 6's `sessionFilesClient` pattern:
six hooks (`useSessionActivity`, `useSessionAgentRoster`, `useAgentConversation`, `useAcpReplay`,
`useAcpToolCallDetail`, `useSessionNotifications`) and nine components, including
`SessionsDrawerScreen`, which builds a third participant client so a LiveKit-routed session reaches
`activity.ActivityService` on the coder's own participant.

The Cypress fakes are split per service — `acpReplay.ts`, `agentConversationBackend.ts`,
`sessionAgentRosterBackend.ts` and `sessionNotificationFeed.ts` move to the new coordinates,
`connectionServiceBackend.ts` keeps `ListSubagents`, which stayed.

**A bundle and a daemon must now be upgraded together for a seventeenth time.** An older bundle
against a newer daemon cannot attach an agent, open a conversation, or render the activity,
notification and replay panes.

Recorded rather than fixed: `SessionMainPaneProps` takes **42 props**. This change re-pointed several
and added none, but a 42-prop interface is where a coordinate change becomes a 42-line diff — see
[`docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md`](../../../../docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md).

Docs: [session-agent-tree.md](../session-agent-tree.md),
[session-agent-conversation.md](../session-agent-conversation.md).
Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
