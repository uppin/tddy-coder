# tddy-session-web documentation

The session package, filled in four `#web-split` nodes: the seams and helpers (#851), the session
runtime and terminals (#860), the inspector, agents and code pane (#861), and the drawer (#862).
`tddy-web` holds no session component any more.

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

## Inspector, agents and code pane (#861)

| Area | Source | Reference |
|---|---|---|
| Main pane | `src/SessionMainPane.tsx`, `src/sessionMainPaneProps.ts`, `src/sessionBaseView.ts`, `src/workflowViews.tsx` | over the 500-line budget: [code-issues/](code-issues/) |
| Inspector | `src/SessionInspectorDrawer.tsx`, `src/SessionInspectorHeader.tsx`, `src/InspectorTabs.tsx`, the tabs (`SessionInspectorDetailsTab`, `SessionInspectorActivityTab`, `SessionFilesTab`, `SessionToolsTab`, `SessionUsageTab`, `SessionScreenSharingTab`, `SessionVncTab` + `VncPassphraseDialog`) and their state helpers | [inactive-session-activities.md](inactive-session-activities.md) |
| Agents | `src/SessionAgentRosterPane.tsx`, `src/SessionAgentTree.tsx` (`agentTree`), `src/AgentPicker.tsx`, `src/SessionAgentConversationLayer.tsx`, `src/SessionAgentConversationPane.tsx`, `src/useAgentConversation.ts`, `src/SessionActivitySidebar.tsx`, `src/SessionWithActivitySidebar.tsx` | [session-agent-tree.md](session-agent-tree.md), [session-agent-conversation.md](session-agent-conversation.md) |
| Code pane | `src/code/` (`WorktreeCodePane`, code navigation, indexing indicator, restructure plan dialog, workflow files) | [code-navigation.md](code-navigation.md), [indexing-indicator.md](indexing-indicator.md) |

`PrStackPresenterRoomAcceptance` stays in `tddy-web`: its assertions are about the app's HTTP
transport minting the presenter-room token through the auth gate.

## Drawer and project home (#862)

| Path | Contents |
|---|---|
| `src/SessionsDrawerScreen.tsx`, `src/SessionsDrawerScreenParts.tsx`, `src/SessionsDrawerNotices.tsx`, `src/useSessionsDrawer*.ts` | the `#/sessions` screen and the hooks it is cut into (activation, attach claims, delete selection, inspector, owning host, pane actions, project homes, runtime attachment and registry, clients, projects, session list) |
| `src/SessionDrawer.tsx`, `src/SessionDrawerGroups.tsx`, `src/SessionDrawerItem.tsx`, `src/SessionDrawerSeparator.tsx`, `src/SessionProjectGroup.tsx`, `src/projectGroupCollapse.ts`, `src/SessionIndicatorDot.tsx`, `src/sessionIndicatorDotStyles.ts` | the drawer list |
| `src/ProjectHomeEntry.tsx`, `src/useEnsureProjectHome.ts`, `src/useProjectHomeSetup.ts` | project home |
| `src/sessionManager.ts`, `src/resumedSessions.ts`, `src/useResumedSessionLiveness.ts`, `src/inspectorState.ts`, `src/jailedCodebaseHalf.ts`, `src/SessionDetailPane.tsx` | the session list store and drawer state |
| `src/StatusBar.tsx`, `src/SessionTrafficStrip.tsx`, `src/SessionHostStatsFooter.tsx`, `src/UploadProgressIndicator.tsx`, `src/useAttachedSessionTraffic.ts`, `src/useMeterSnapshot.ts`, `src/rpc/sessionTraffic.tsx` | the status strip; traffic through the `SessionTrafficProvider` seam (`tddy-web`'s `AppClientsPorts` supplies the meter registry and the LiveKit room lookup) |

`bunfig.toml` and `src/test-utils/dom-preload.ts` live here now; the root `bunfig.toml` preloads it.
The drawer specs run without a stylesheet, as they did in `tddy-web` (two layout specs import
`theme.css` themselves). Specs whose subject is app wiring — real LiveKit routing, real token HTTP,
the app shell and URL store — stay in `tddy-web`.

Product behaviour: [docs/ft/web/session-drawer.md](../../../docs/ft/web/session-drawer.md).

- **Changeset history**: [changesets/](changesets/)

## Tests

`bun test src` — the helpers' suites, the seam rules and the boundary rule (builders and the
`toContainSessionIdsInOrder` matcher in `test-utils/`) and `bun run cypress:component` — the runtime and
terminal specs (#860), the inspector, agent and code-pane specs (#861) and the drawer-driven specs (#862). CI: `.github/workflows/web.yml` (`Web tests` shards run its suites; `Web lint`
excludes it until its moved drawer specs typecheck — `docs/dev/todo/2026-11-07-session-web-specs-do-not-typecheck.md`).
