# Web Changelog

Release note history for the Web product area.

## 2026-03-28 — Connection screen: workflow recipe (TDD / Bugfix)

- **ConnectionScreen**: Per project, **Start New Session** includes a **Workflow recipe** control (**TDD** vs **Bugfix**); the selected value is sent as **`recipe`** on **`StartSession`** / **`StartSessionRequest`** (proto **`connection.proto`** / **`remote.proto`**).
- **Vite**: **`tddy-livekit-web`** resolves via **`resolve.alias`** to package source for component tests and dev without a prior **`dist`** build.
- **Feature docs**: [workflow-recipes.md](../coder/workflow-recipes.md), [web-terminal.md](web-terminal.md) (Connection screen).

## 2026-03-28 — Connection chrome: status dot, menu, Stop

- **`GhosttyTerminalLiveKit`** **`connectionOverlay`**: Top-left **build id**; top-right **status dot** with **`data-connection-status`** (values **`connecting`**, **`connected`**, **`error`**); dot menu lists **Disconnect** and **Terminate** when **`onTerminate`** is provided (SIGTERM). **Stop** (`data-testid="terminal-stop-button"`) sits bottom-right and enqueues **0x03** on the same terminal input queue as keyboard interrupt. Implementation: **`ConnectionTerminalChrome`**, **`dataConnectionStatusValue`**, **`connectionChromeMarkers`**; pulse animation for **`connecting`** respects **`prefers-reduced-motion`**; menu dismisses on outside click or **Escape**.
- **ConnectedTerminal** (**standalone** and **ConnectionScreen**): During JWT fetch, the fullscreen container shows the same chrome so the primary loading indicator is the dot (not a **`livekit-status`**-only screen).
- **ConnectionScreen**: Connected state carries **`sessionId`**; **Terminate** in the dot menu invokes **`SignalSession`** (SIGTERM), aligned with the session table **Signal** dropdown.
- **Tests**: Bun **`connectionChromeStatus.test.ts`**; Cypress component specs **`App.cy.tsx`**, **`ConnectionScreen.cy.tsx`**, **`GhosttyTerminalLiveKit.cy.tsx`**.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Connection chrome; Fullscreen terminal session chrome).

## 2026-03-28 — Connection screen: host selection (eligible daemons)

- **ConnectionService**: **`ListEligibleDaemons`** lists daemon instances for the Host dropdown; **`StartSession`** and **`SessionEntry`** carry **`daemon_instance_id`**.
- **ConnectionScreen**: Per-project **Host** dropdown (populated after auth), **Host** column on project and **Other sessions** tables, **`daemonInstanceId`** passed on **`StartSession`**. Cypress component tests cover dropdown, default local selection, encoded **`StartSession`** body, column display, and single-daemon case.
- **Deferred**: LiveKit common-room peer discovery and cross-daemon spawn routing remain future work; the daemon rejects non-local **`daemon_instance_id`** until that ships.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Daemon mode: Connection screen; Eligible daemons and host selection).

## 2026-03-24 — Connection screen: delete inactive sessions

- **ConnectionScreen**: Inactive session rows include **Resume** and **Delete** (project session tables and **Other sessions**). **Delete** uses a browser **confirm**, invokes **`DeleteSession`**, refreshes the session list after success, and surfaces RPC errors in the shared connection error area. Active rows show **Connect** and **Signal** only.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Inactive session deletion).

## 2026-03-24 — Connection screen: session table ordering

- **ConnectionScreen**: Project session tables (`sessions-table-{projectId}`) and **Other sessions** (`sessions-table-orphan`) render rows in a fixed display order: active sessions first, then inactive; within each group, newer **`createdAt`** (ISO-8601) before older; ties and unparsable timestamps resolve by **`sessionId`** lexicographically. Implementation: **`sortSessionsForDisplay`** in `packages/tddy-web/src/utils/sessionSort.ts`, applied after filtering by project or orphan set. **Bun** unit tests (`sessionSort.test.ts`) and **Cypress** component tests assert order when **`ListSessions`** returns a non-canonical sequence.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Daemon mode: Connection screen).

## 2026-03-22 — Connection screen: connected participants (common room)

- **Daemon config**: `livekit.common_room` names a shared LiveKit room; **`/api/config`** exposes it as **`common_room`** alongside **`livekit_url`**.
- **ConnectionScreen** (daemon mode): After GitHub auth, the browser joins that room as **`web-{githubLogin}`** and shows a **Connected participants** table (identity, role, joined time, metadata), updated live via LiveKit events. Terminal full-screen session view is unchanged; the presence connection stays active while the terminal is open.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Daemon mode: shared LiveKit room).

## 2026-03-22 — Local web dev: `./web-dev`

- **Feature doc**: [local-web-dev.md](local-web-dev.md) describes the daemon + Vite flow, **`DAEMON_CONFIG`**, temp YAML, CLI pass-through, **`DAEMON_PORT`** for the proxy, and **`fuser`** port cleanup.
- **E2E contract tests**: `packages/tddy-e2e` includes static checks for the repo-root **`web-dev`** script (`cargo test -p tddy-e2e web_dev`).

## 2026-03-21 — Terminal: coder left the room

- **GhosttyTerminalLiveKit**: When the LiveKit **server/coder** participant disconnects (`ParticipantDisconnected` for `serverIdentity`), input to the RPC stream stops, the terminal is dimmed and non-interactive (`data-session-active="false"` on `GhosttyTerminal`), and a full-area banner **`terminal-coder-unavailable`** explains that the session ended.

## 2026-03-21 — Mobile keyboard: Ctrl+letter sends control bytes

- **GhosttyTerminalLiveKit**: `handleMobileKeyDown` maps **Ctrl+A–Z** to bytes 0x01–0x1A (e.g. **Ctrl+C → 0x03**). Previously only `onInput` ran, so **Ctrl+C** appeared as the letter **`c`** (0x63).
- **Connection overlays**: **Ctrl+C** / **Disconnect** / **build id** render **inside** `GhosttyTerminalLiveKit` (`connectionOverlay` prop) **above** the terminal (`z-index: 100`, DOM after canvas) and call the same **`enqueueTerminalInput`** queue as keyboard — fixes clicks hitting the canvas and only logging `'c'`/`'v'` from Ghostty `onData`.

## 2026-03-21 — Daemon: `--mouse` for spawned tools

- **tddy-daemon**: Spawns `tddy-*` with **`--mouse`** by default (Virtual TUI / browser touch). Config **`spawn_mouse`** (default `true`) in daemon YAML disables it when set to `false`. **dev.daemon.yaml** documents the option.

## 2026-03-21 — Daemon + LiveKit: wait for feature input

- **tddy-coder `--daemon` + LiveKit**: New sessions no longer use a placeholder `"feature"` prompt, which skipped **Feature input** and jumped straight into plan / first clarification. The workflow now blocks until feature text is submitted from the Virtual TUI (browser terminal over LiveKit), matching headless stdin (`/dev/null` from the spawner).

## 2026-03-21 — Connection screen: tool + debug per session

- **ConnectionScreen**: **Tool**, **backend**, and **debug logging** (browser terminal) are configured **per project** in each accordion—only for that session/connection, not stored on the project. **Other sessions** has its own debug checkbox for Connect/Resume.

## 2026-03-21 — Dev daemon: tool dropdown entries

- **dev.daemon.yaml**: `allowed_tools` includes `tddy-coder` and `tddy-tools` (debug/release) so the connection screen **Tool** dropdown lists them alongside `tddy-demo`.

## 2026-03-21 — Connection screen: backend dropdown

- **ConnectionScreen**: Backend select (Claude, Claude ACP, Cursor, Stub); value sent as `agent` on `StartSession`. **Per session:** choice applies only to that spawn, not stored on the project.

## 2026-03-21 — Feature docs: token auth PRD wrapped

- **Consolidated** [PRD: Server-side token auth via Connect-RPC](1-WIP/archived/PRD-2026-03-14-client-side-token-auth.md) into [web-terminal.md](web-terminal.md) and this changelog. Source PRD moved to `docs/ft/web/1-WIP/archived/`.

## 2026-03-21 — Connection screen: projects and sessions

- **ConnectionScreen**: Lists projects (`ListProjects`), inline create project (`CreateProject` with optional path-under-home), accordion sections per project with sessions filtered by `projectId`, **Start New Session** per project (`StartSession` with `project_id`). Orphan sessions section when `project_id` is unknown to the list.
- **Removed**: Manual repository path field; work is always scoped to a project.

## 2026-03-18 — Terminal Mobile UX: Keyboard, Resize, Touch, Build ID

- **Keyboard-aware resize**: useVisualViewport hook tracks `visualViewport.height`; terminal container resizes when virtual keyboard opens or closes.
- **Manual keyboard button**: Floating "Keyboard" button at bottom center on mobile; auto-focus disabled; button hides when keyboard open.
- **Focus prevention**: preventFocusOnTap + readonly textarea prevent keyboard from opening on tap; keyboard opens only via Keyboard button.
- **Touch/SGR forwarding**: Capture-phase touch handlers send SGR mouse sequences for interactive TUI (vim, htop); tap-to-click works.
- **Build ID**: Prebuild script generates timestamp; overlay shows build ID for cache verification on mobile.
- **HMR counter**: Dev-only overlay shows hot-reload count when running under Vite.

## 2026-03-17 — Terminal UX: Fullscreen, Auto-Focus, Adaptive Size, Touch/Mouse

- **Fullscreen**: ConnectedTerminal fills 100% viewport. Overlay: Disconnect and Ctrl+C buttons.
- **Auto-focus**: Keyboard focus is set on the terminal when ready.
- **Adaptive size**: FitAddon auto-sizes to container. Resize escape sequence `\x1b]resize;{cols};{rows}\x07` flows to virtual TUI.
- **Touch/mouse**: `--mouse` flag on tddy-coder enables mouse capture. GhosttyTerminal encodes SGR mouse sequences and forwards via onData. Click-to-select and scroll work.

## 2026-03-14 — Token Fetch via Connect-RPC

- **Token form**: Identity, url, room. Connect-RPC client fetches tokens from server (GenerateToken, RefreshToken).
- **getToken prop**: GhosttyTerminalLiveKit accepts getToken for token refresh 1 minute before expiry.
- **Backward compat**: token prop still works; Storybook and e2e pass pre-generated tokens via URL params.

## 2026-03-13 — Ghostty Terminal Integration via LiveKit

- **GhosttyTerminal**: React component wrapping ghostty-web for ANSI terminal rendering. Standalone (no LiveKit dependency); used by Storybook and LiveKit-connected story.
- **GhosttyTerminalLiveKit**: Storybook story that connects to tddy-demo via LiveKit, streams TerminalOutput to GhosttyTerminal, pipes onData back as TerminalInput.
- **TerminalService**: New RPC in tddy-livekit (StreamTerminalIO) — bidirectional streaming of terminal bytes over LiveKit data channels.
- **tddy-demo LiveKit args**: `--livekit-url`, `--livekit-token`, `--livekit-room`, `--livekit-identity` wire terminal byte capture to LiveKit participant.
- **E2E test**: Cypress startTerminalServer/stopTerminalServer tasks; asserts streamed bytes and terminal buffer content through full stack.
- **Supersedes**: WebSocket-based web-terminal approach; streaming tddy-coder TUI is now implemented via LiveKit.
