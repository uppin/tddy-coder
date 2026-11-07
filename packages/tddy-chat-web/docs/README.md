# tddy-chat-web documentation

Agent chat and everything that renders an agent's conversation:

- **Chat** — `AgentChat` (recipe-agnostic: a `placeholder` prop, `agent-chat-*` test ids),
  `LiveAgentChatView`, `AgentTranscriptView`, with the transports behind one result shape
  (`UseAgentChatResult`): `useAgentChat` (the `TddyRemote` presenter stream) and `useAcpSession` /
  `useAcpSessionOverClient` (the `AcpService.Session` stream; its frames and the
  `request_permission` → question mapping are in `acpSessionMessages.ts`).
- **Replay and tool detail** — `useAcpReplay` (paged transcript replay, `acpReplayProjection`),
  `useAcpToolCallDetail`, `toolCallPresentation`, `chatEntryPresentation`, `acpAgentMerge`.
- **Activity** — `agentActivityRegistry` (chat state the session views observe; owning it here is
  what removes the chat → sessions cycle), `SessionActivitiesPane`, `AgentActivityOverlay`,
  `AgentActivityDetailDialog`, `useSessionActivity`.
- **Subagents and permissions** — `SubagentConversationDialog`, `PermissionRequestEntry`,
  `TranscriptPermissionRequest`, `pendingPermission`, `useAnswerAgentPermission`,
  `sessionRosterHalf`.
- **Helpers** — `chatTranscript` (the exported timeline), `scrollFollow` / `useTailFollow`.

It imports nothing from `tddy-web` and nothing from LiveKit (`src/packageBoundary.test.ts`): rooms are
reached through `tddy-client-web`'s room-RPC port, the stream queue is `tddy-rpc-web`'s `AsyncQueue`.
The session layers that host chat (`SessionAgentConversationLayer`, the roster) stay in `tddy-web`.

Diagnostics use `tddyDebug` namespaces: `tddy:presenter:stream`, `tddy:acp:session`,
`tddy:acp:replay` (enable with `localStorage.debug = 'tddy:*'`).

- **Product behaviour**: [docs/ft/web/session-drawer.md § Agent Chat](../../../docs/ft/web/session-drawer.md),
  [docs/ft/coder/acp-protobuf-rpc.md](../../../docs/ft/coder/acp-protobuf-rpc.md),
  [docs/ft/web/chat-package.md](../../../docs/ft/web/chat-package.md)
- **Changeset history**: [changesets/](changesets/)

## Tests

`bun test src` (replay projection, tool-call presentation, activity registry, pending permissions,
scroll follow, transcript export, the boundary rule) and `bun run cypress:component` — 13 component
specs from the package's own `cypress.config.ts`, mounted with `cy.mountWithClients`; live-chat specs
use `mountWithChatRoom` (a room-RPC port double with the serving participant present). CI:
`.github/workflows/web.yml` (`Web lint` typechecks it; the `Web tests` shards run its suites).
