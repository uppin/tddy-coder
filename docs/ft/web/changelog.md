# Web Changelog

Release note history for the Web product area.

**Merge hygiene:** [Changelog merge hygiene](../../dev/guides/changelog-merge-hygiene.md) — newest **`##`** first; **distinct titles** when two releases share a date; single-line bullets; do not edit older sections for unrelated work.

## 2026-04-11 — Connection screen: multi-host eligible daemons (LiveKit common room)

- **`tddy-web`**: **ConnectionScreen** sorts **ListEligibleDaemons** for the Host dropdown (**local** first, then **`instance_id`**); **StartSession** sends the selected **`daemonInstanceId`** when the daemon lists multiple eligible hosts. Cypress **ConnectionScreen** covers multi-row host list and multi-session disconnect scoping. **Feature docs**: [web-terminal.md](web-terminal.md), [livekit-peer-discovery.md](../daemon/livekit-peer-discovery.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-10 — ParticipantList Codex OAuth presence

- **tddy-web**: `parseCodexOAuthPending` in `ParticipantList` — "Codex sign-in" column with `ExternalLink` icon for pending OAuth; `codexOauthMetadata.ts` parser. Cypress `ParticipantList.cy.tsx` OAuth assertions; Bun `codexOauthMetadata.test.ts`. Feature doc: [codex-oauth-web-relay.md](codex-oauth-web-relay.md).

## 2026-04-06 — Codex OAuth web relay (dialog + docs)

- **tddy-web**: **`CodexOAuthDialog`** — modal (**`codex-oauth-dialog`**), dismiss (**`codex-oauth-dismiss`**), sandboxed authorize **iframe** when **`embeddingBlocked`** is false; **embedding-blocked** panel (**`codex-oauth-embedding-fallback`**) with external link (**`noopener`**, **`noreferrer`**) when **`embeddingBlocked`** is true. Cypress **`CodexOAuthDialog.cy.tsx`**, **`CodexOAuthIframeFallback.cy.tsx`**.
- **Docs**: **[codex-oauth-web-relay.md](codex-oauth-web-relay.md)**; package **[codex-oauth-dialog.md](../../../packages/tddy-web/docs/codex-oauth-dialog.md)**. Cross-package: **[docs/dev/changesets.md](../../dev/changesets.md)**; **[packages/tddy-web/docs/changesets.md](../../../packages/tddy-web/docs/changesets.md)**.

## 2026-04-05 — Connection screen: concurrent terminal attachments

- **`tddy-web`**: **`ConnectionScreen`** stores **`sessionAttachments`** (**`Map<sessionId, LiveKitConnectionParams>`**); **Connect** / **Resume** / **Start** merge attachments without dropping prior sessions; **`focusedSessionIdFromPathname`** aligns **fullscreen** focus with **`/terminal/{sessionId}`**; floating **overlay** / **mini** render one **`ConnectedTerminal`** per attachment under **`data-testid="connection-attached-terminal-{sessionId}"`**; inactive **`ListSessions`** rows prune matching attachments only; **Disconnect** removes one entry; **popstate** to `/` clears all. Pure helpers: **`multiSessionState.ts`**, **`multiSessionPresentation.ts`**. Bun **`multiSessionState.test.ts`**, **`multiSessionPresentation.test.ts`**; Cypress **`ConnectionScreen.cy.tsx`** (multi-session scenarios).
- **Feature doc**: [web-terminal.md](web-terminal.md) (daemon mode — concurrent attachments, attach behavior). **Dev reference**: [terminal-presentation.md](../../../packages/tddy-web/docs/terminal-presentation.md). Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Connection screen: workflow recipe control

- **`ConnectionScreen`**: **Workflow recipe** lists **`tdd`**, **`tdd-small`**, **`bugfix`**, **`free-prompting`**, **`grill-me`**, **`review`**, and **`merge-pr`**; the default selection for **Start New Session** is **`free-prompting`**. **`StartSession.recipe`** sends the selected CLI name to the daemon.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Projects — collapsible sections). Package **[tddy-web](../../packages/tddy-web/docs/changesets.md)**.

## 2026-04-05 — Terminal connection status bar (chrome layouts)

- **tddy-web**: **`TerminalConnectionStatusBar`** hosts **`ConnectionTerminalChrome`** with **`chromeLayout="statusBar"`** above **`GhosttyTerminal`** in **`GhosttyTerminalLiveKit`**, **`ConnectionScreen`**, and standalone connect. **`connectionChromePlacement`** **`floating`** | **`none`** selects full vs compact bar (build id + fullscreen vs dot-focused chrome for overlay / mini). **`terminalStatusBarLayout`** geometry helpers with Bun tests; Cypress **`GhosttyTerminalLiveKit.cy.tsx`** shares those helpers. **`connectionTerminalChromeDotStyles`** centralizes dot animation styles.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Connection chrome). Dev reference: [terminal-connection-chrome.md](../../../packages/tddy-web/docs/terminal-connection-chrome.md). Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**; package: **[packages/tddy-web/docs/changesets.md](../../../packages/tddy-web/docs/changesets.md)**.

## 2026-04-04 — Connection screen: pending elicitation indicator

- **`SessionEntry`**: **`pending_elicitation`** on **`ListSessions`** (proto field **14**); generated clients expose **`pendingElicitation`**.
- **`ConnectionScreen`**: Session rows show an **Input needed** badge when **`pendingElicitation`** is true; each row sets **`data-pending-elicitation`** on the **`<tr>`**; badge **`aria-label`** for screen readers. Cypress **`ConnectionScreen.cy.tsx`** covers true/false cases.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Pending elicitation on session rows). Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Documentation wrap (worktrees PRD retired)

- **Docs**: WIP PRD for worktrees ConnectionService + web removed from **`docs/ft/web/1-WIP/`**; behavior remains in [worktrees.md](worktrees.md) and [web-terminal.md](web-terminal.md). Session validation report copies under **`plans/`** for terminal reconnect removed. Cross-package note: **[docs/dev/changesets.md](../../dev/changesets.md)**.

## 2026-04-05 — Connection screen: per-table bulk session selection and delete

- **tddy-web**: **`sessionSelection`** helpers (**`computeHeaderCheckboxState`**, **`toggleSelectAllForTable`**, **`toggleRowInTableSelection`**); **`ConnectionScreen`** stores selection per project table and **`orphan`** table; header checkbox **indeterminate** for partial selection; **Delete selected** (**`bulk-delete-selected-{tableKey}`**) with one **`window.confirm`** (count in copy), sequential **`DeleteSession`**, **`ListSessions`** refresh and selection clear on full success; on failure after partial deletes, **`setError`**, **`ListSessions`**, and selection pruned to ids still listed. **`sessionSelection`** has no **`console`** in shipped helpers; **`ConnectionScreen`** bulk diagnostics use **`import.meta.env.DEV`**. **Bun** **`sessionSelection.test.ts`**; Cypress **`ConnectionScreen.cy.tsx`** (select all, indeterminate, per-id deletes, cancel confirm, orphan vs project independence).
- **Feature doc**: [web-terminal.md](web-terminal.md) (**Session deletion** — per-table selection and bulk delete).
- **Dev guide**: [testing.md](../../dev/guides/testing.md) (**Rust workspace: `./verify` vs plain `cargo test`** — **`tddy-acp-stub`** prerequisite for full **`cargo test`**).
- **Package history**: [packages/tddy-web/docs/changesets.md](../../../packages/tddy-web/docs/changesets.md).

## 2026-04-04 — Terminal reconnect overlay (presentation + routing)

- **tddy-web**: **`terminalPresentation`** pure helpers (**`nextPresentationFromAttach`**, **`applyOverlayPreviewClickToFull`**, **`applyDedicatedTerminalBackToMini`**, **`reconcileReconnectOverlayInstances`**, **`defaultTerminalMiniOverlayPlacement`**). **`ConnectionScreen`** branches **new** vs **reconnect** attach: **Start** / **Connect** → **`full`** + route push when applicable; **Resume** → **`overlay`** (floating **160px** preview, **`terminal-reconnect-overlay-root`**) without automatic route push; **Expand** → **`full`** + push; **Back** → **`mini`**. **`ConnectedTerminal`** supports **`fullscreen`** | **`overlay`** | **`mini`** layouts. **`terminalDeepLinkSessionPath`** aligns with **`terminalPathForSessionId`**. **`navigatePath`** uses the shell **`onNavigate`** callback for **push** so **`App`** `path` stays aligned. First failed **`ListSessions`** surfaces **`setError`**. Bun tests (**`terminalPresentation.test.ts`**, **`appRoutes.test.ts`**, **`ConnectionScreen.test.tsx`**); Cypress **`ConnectionScreen.cy.tsx`** (resume omits **`history.pushState`**; connect performs push).
- **Feature docs**: [web-terminal.md](web-terminal.md) (URL routes, attach behavior); dev reference [terminal-presentation.md](../../../packages/tddy-web/docs/terminal-presentation.md).

## 2026-04-04 — Worktrees manager (library + RPC + UI)

- **`tddy-daemon`**: **`worktrees`** module — **`git worktree list`** parsing, **`WorktreeStatsCache`** with JSON persistence under **`TDDY_PROJECTS_STATS_ROOT`** (default **`~/.tddy/projects`**), lexical path policy, **`git worktree remove`** for non-primary trees listed by Git. **ConnectionService** exposes **`ListWorktreesForProject`** and **`RemoveWorktree`** (tests **`worktrees_acceptance`**, **`worktrees_rpc`**).
- **`tddy-service` / proto**: **`WorktreeRow`**, **`ListWorktreesForProject`**, **`RemoveWorktree`** on **`ConnectionService`**.
- **`tddy-web`**: **`WorktreesAppPage`** loads projects/daemons, **Refresh stats**, table rows, and delete via Connect; **`WorktreesScreen`** (stale hint, empty state). Cypress **`WorktreesScreen.cy.tsx`** (mocked rows).
- **Feature docs**: [worktrees.md](worktrees.md); [web-terminal.md](web-terminal.md#worktrees-manager-scaffolding). Package: [worktrees.md](../../../packages/tddy-daemon/docs/worktrees.md).

## 2026-04-04 — Daemon URL routes: `/terminal/{sessionId}`, SPA fallback, standalone cleanup

- **tddy-web**: **`appRoutes`** helpers (`/terminal/:id`, `/`, `/auth/callback`). **`ConnectionScreen`** (daemon mode) **pushes** the terminal path after Start/Connect/Resume, **replaces** with `/` on Disconnect, handles **popstate** for Back, deep-link attach on load, and unknown-session UI. **`App`** (standalone) **replaces** a stray **`/terminal/...`** URL with **`/`** so standalone keeps the query-param connect model.
- **tddy-coder**: **`web_bundle_acceptance`** asserts **`GET /terminal/...`** returns the SPA **`index.html`** (same stack as **`serve_web_bundle`** SPA fallback).
- **Feature docs**: [web-terminal.md](web-terminal.md) (URL routes — daemon mode).

## 2026-04-03 — Interrupt: TUI Stop pane; web Stop button removed

- **tddy-web**: **`ConnectionTerminalChrome`** no longer renders a bottom-right **Stop** button or **`onStopInterrupt`**. Interrupt is the ratatui **Stop** pane (red **U+25A0**) beside the Enter strip; the browser forwards SGR mouse to the virtual TUI (same **0x03** path as **Ctrl+C**).
- **Feature docs**: [web-terminal.md](web-terminal.md) (Connection chrome); [TUI Stop control](../coder/tui-status-bar.md#mouse-mode-stop-control).

## 2026-04-03 — Web terminal documentation: TUI mouse Enter affordance

- **Docs**: [web-terminal.md](web-terminal.md) (**Connected Terminal UX**) describes the **three-column** Enter affordance to the right of the prompt (starts below the status bar; box drawing + **U+23CE** on the first prompt text row), aligned with [TUI status bar — mouse mode](../coder/tui-status-bar.md#mouse-mode-enter-control).

## 2026-04-03 — Session workflow files, project/worktree matching, delete hardening

- **`connection.proto`**: **`ListSessionWorkflowFiles`** and **`ReadSessionWorkflowFile`** on **`ConnectionService`** for allowlisted workflow artifacts under the daemon-resolved session directory.
- **tddy-daemon**: Allowlisted listing and UTF-8 reads (**`session_workflow_files`**, tests **`session_workflow_files_rpc`**). **`sessions_base`** for mapped users is the Tddy data root (**`~/.tddy`**), aligning RPC paths with **`tddy-coder`** session directories. **`DeleteSession`** stops a live tool process when needed, then removes the session directory; directories without readable **`.session.yaml`** can still be removed when the path is valid.
- **tddy-web**: **`SessionWorkflowFilesModal`** loads list/read RPCs; **`SessionMoreActionsMenu`** → **Show files**; **`SessionFilesPanel`** + **`sessionWorkflowPreview`**. Cypress **`SessionWorkflowFiles.cy.tsx`**; Bun tests for preview and **`sessionProjectTable`** (unscoped sessions match a project when **`repoPath`** equals **`mainRepoPath`** or sits under it—longest-prefix wins). **Delete** (trash) is available for **active** and **inactive** rows; confirmation copy describes stop-then-delete.
- **Feature docs**: [web-terminal.md](web-terminal.md); [connection-service.md](../../packages/tddy-daemon/docs/connection-service.md).

## 2026-03-29 — Connection screen: backend options from `ListAgents`

- **ConnectionScreen**: **Backend** (per project) is populated from **`ListAgents`**; option values are agent **`id`** strings from daemon **`allowed_agents`**, with labels from the RPC (blank optional labels resolve to **`id`** on the server). **Start New Session** requires a selected backend when the list is non-empty. The default selection is the first RPC entry unless a stored choice for that project still exists in the list.
- **RPC load**: **`ListTools`** and **`ListAgents`** run together after auth; either failure clears both dropdowns and shows the shared connection error message.
- **Tests**: Bun helpers in **`agentOptions.ts`**; Cypress component coverage for dynamic backend options and **`ListAgents`** intercepts.
- **Feature docs**: [web-terminal.md](web-terminal.md) (Connection screen), [local-web-dev.md](local-web-dev.md) (dev YAML tools + agents).

## 2026-03-29 — Connection chrome: immersive status, fullscreen, Terminate confirm

- **`GhosttyTerminalLiveKit`** with **`connectionOverlay`**: The **`livekit-status`** text strip stays out of layout for **`connecting`** / **`connected`**; **`data-connection-status`** on the dot carries phase; errors use **`livekit-error`**. Policy helper: **`shouldShowVisibleLiveKitStatusStrip`** (`packages/tddy-web/src/lib/liveKitStatusPresentation.ts`).
- **`ConnectionTerminalChrome`**: Top-right **`terminal-fullscreen-button`** toggles document fullscreen on the terminal target via **`browserFullscreen`** (standard API + prefixed enter/exit). **`confirmRemoteSessionTermination`** wraps **`window.confirm`** before **`onTerminate`** (shared copy for remote session termination).
- **Tests**: Bun specs for **`browserFullscreen`**, **`liveKitStatusPresentation`**, **`remoteTerminateConfirm`**; Cypress component coverage for chrome placement, fullscreen stub, and terminate flows; e2e contracts assert overlay chrome without a visible **`livekit-status`** row during normal connection.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Connection chrome; Fullscreen terminal session chrome).

## 2026-03-28 — Connection screen: workflow recipe (TDD / Bugfix)

- **ConnectionScreen**: Per project, **Start New Session** includes a **Workflow recipe** control (**TDD** vs **Bugfix**); the selected value is sent as **`recipe`** on **`StartSession`** / **`StartSessionRequest`** (proto **`connection.proto`** / **`remote.proto`**).
- **Vite**: **`tddy-livekit-web`** resolves via **`resolve.alias`** to package source for component tests and dev without a prior **`dist`** build.
- **Feature docs**: [workflow-recipes.md](../coder/workflow-recipes.md), [web-terminal.md](web-terminal.md) (Connection screen).

## 2026-03-28 — Connection chrome: status dot, menu, Stop

- **`GhosttyTerminalLiveKit`** **`connectionOverlay`**: Top-left **build id**; top-right **status dot** with **`data-connection-status`** (values **`connecting`**, **`connected`**, **`error`**); dot menu lists **Disconnect** and **Terminate** when **`onTerminate`** is provided (SIGTERM). **Stop** (`data-testid="terminal-stop-button"`) sits bottom-right and enqueues **0x03** on the same terminal input queue as keyboard interrupt. Implementation: **`ConnectionTerminalChrome`**, **`dataConnectionStatusValue`**; pulse animation for **`connecting`** respects **`prefers-reduced-motion`**; menu dismisses on outside click or **Escape**.
- **ConnectedTerminal** (**standalone** and **ConnectionScreen**): During JWT fetch, the fullscreen container shows the same chrome so the primary loading indicator is the dot (not a **`livekit-status`**-only screen).
- **ConnectionScreen**: Connected state carries **`sessionId`**; **Terminate** in the dot menu invokes **`SignalSession`** (SIGTERM), aligned with the session table **Signal** dropdown.
- **Tests**: Bun **`connectionChromeStatus.test.ts`**; Cypress component specs **`App.cy.tsx`**, **`ConnectionScreen.cy.tsx`**, **`GhosttyTerminalLiveKit.cy.tsx`**.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Connection chrome; Fullscreen terminal session chrome).

## 2026-03-28 — Connection screen: host selection + session workflow status (TUI parity)

- **ConnectionService**: **`ListEligibleDaemons`** lists daemon instances for the Host dropdown; **`StartSession`** and **`SessionEntry`** carry **`daemon_instance_id`**, and **`ListSessions`** includes workflow columns (**`workflow_goal`**, **`workflow_state`**, **`elapsed_display`**, **`agent`**, **`model`**).
- **ConnectionScreen**: Per-project **Host** dropdown (populated after auth), **Host** column on project and **Other sessions** tables, **`daemonInstanceId`** on **`StartSession`**, plus **Goal**, **Workflow**, **Elapsed**, **Agent**, and **Model** columns via **`SessionWorkflowStatusCells`**. Tables use horizontal scroll when needed. Session list polling uses a **2** second interval when any session is active (**`isActive`**), and **5** seconds otherwise; projects still refresh every **5** seconds. Cypress covers host selection and workflow column display.
- **Deferred**: LiveKit common-room peer discovery and cross-daemon spawn routing remain future work; the daemon rejects non-local **`daemon_instance_id`** until that ships.
- **Feature doc**: [web-terminal.md](web-terminal.md) (Daemon mode: Connection screen).
- **Elapsed semantics (QA)**: [web-terminal.md](web-terminal.md) (Daemon mode: Connection screen — **TUI vs web elapsed**).

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
