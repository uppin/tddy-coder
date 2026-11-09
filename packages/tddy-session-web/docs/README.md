# tddy-session-web documentation

The session package, filled in four `#web-split` nodes: the seams and helpers (#851), the session
runtime and terminals (#860), the inspector (#861) and the drawer (#862).

## Seams

Session screens depend on three things the application owns, and reach each only through a slot the
app fills (pinned by `src/sessionSeams.test.ts`):

| Seam | Slot | Filled by |
|---|---|---|
| the app location and its route params | `tddy-client-web`'s `NavigationPort` (`useAppLocation`) and route builders | `tddy-web`'s hash navigation port |
| the screen shell (header, menu, daemon selector, avatar) | `SessionShellProvider` / `SessionShell` (`src/sessionShell.tsx`) | `tddy-web`'s `AppShell` |
| the start-passphrase prompt | `tddy-session-create-web`'s `StartPassphraseProvider` | `tddy-web`'s `HostStartPassphrasePrompt` |

`tddy-web` fills the shell and passphrase slots once, in `AppClientsPorts` (under `AuthProvider`), so
the app and `tddy-web`'s Cypress `mount` both have them; a missing provider throws naming itself.

## Helpers

| Path | Contents |
|---|---|
| `src/lib/` | `sessionIndicator` (the drawer dot), `sessionNotificationRegistry`, `sessionParticipantMetadata`, `sessionsRefreshBridge`, `sessionReadsLocalPaths` |
| `src/utils/` | `crossHostSessions`, `projectHomeGroups`, `sessionProjectGroups`, `sessionDrawerLabel` (mirrored case for case by `tddy-changeset`'s `session_display_label`), `sessionSort`, `sessionDisplay`, `sessionSelection`, `connectionStatusForSession`, `sessionStackGroups`, `sessionProjectTable` |
| `src/rpc/` | `trafficMeter`, `daemonTransportFlavour`, `livekitPing`, `useSessionNotifications` (token through the auth port) |

## Runtime and terminals (#860)

| Path | Contents |
|---|---|
| `src/SessionRuntime.tsx`, `src/SessionChildRuntime.tsx`, `src/DormantSessionRuntime.tsx`, `src/DormantAgentTerminal.tsx` | a live session's tab strip and pane stack, a child session's, and a stopped session's |
| `src/sessionRuntimeRegistry.ts`, `src/sessionRuntimeProps.ts` | the per-session runtime registry (byte deltas, metadata) and the runtime's props |
| `src/GrpcSessionTerminal.tsx`, `src/useSessionTerminalFeed.ts`, `src/useSessionTerminals.ts`, `src/SessionTerminalTabs.tsx`, `src/NewTerminalMenu.tsx`, `src/useTmuxSessions.ts`, `src/tmuxSessionName.ts` | a session terminal over the terminal RPC, the tab strip and tmux sessions |
| `src/useSessionAttachment.ts`, `src/attachClaim.ts`, `src/useConversationHostAttachment.ts`, `src/rpc/connections/{hostServedSession,useConnectionStatus}.ts` | attaching a session over its host connection |
| `src/SessionPaneChrome.tsx`, `src/SessionConnectionOverlay.tsx`, `src/TerminalControlOverlay.tsx`, `src/useSessionPaneFullscreen.ts`, `src/useAgentTerminalFocus.ts`, `src/agentConversationTabs.ts`, `src/lib/terminalGridMeasure.ts`, `src/useChildSessions.ts` | pane chrome, the handshake and control overlays, and the helpers only the runtime uses |

The runtime's specs run on the clients-port double: a session connection is a host-connection double
whose `openSession` is the production `openHostServedSession`. Token minting for a room-backed session
is `tddy-livekit-web`'s (`sessionConnection.test.ts` — "joins the named room with a token minted for a
browser observer identity").

Product behaviour: [docs/ft/web/session-drawer.md](../../../docs/ft/web/session-drawer.md).

- **Changeset history**: [changesets/](changesets/)

## Tests

`bun test src` — the helpers' suites, the seam rules and the boundary rule (builders and the
`toContainSessionIdsInOrder` matcher in `test-utils/`) and `bun run cypress:component` — the runtime and
terminal specs (#860). CI: `.github/workflows/web.yml` (`Web tests` shards run its suites; `Web lint`
excludes it for the one `tddy-tauri-web` error it compiles through `tddy-terminal-web`).
