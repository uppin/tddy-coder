# Web Changelog

Release note history for the Web product area.

**Merge hygiene:** [Changelog merge hygiene](../../dev/guides/changelog-merge-hygiene.md) — newest **`##`** first; **distinct titles** when two releases share a date; single-line bullets; do not edit older sections for unrelated work.

## 2026-07-06 — Per-session model selection

- The **Create session** form now lets the operator pick the backend **model** for **tool** (tddy-coder) sessions, not just claude-cli. The model list is fetched on demand for the selected agent via the new `ListAgentModels` RPC — enumerated straight from the agent command where possible (`cursor --list-models`, ACP `available_models`) and a curated tddy-core list for `claude`/`codex`. Changing the agent repopulates the options and resets to that backend's default; the chosen model is threaded to `tddy-coder --model` for the whole session. See [tool-session-model-selection.md](tool-session-model-selection.md).
- If a backend's model probe fails (not logged in, binary missing), the form shows an inline error and disables Create for that agent — no silent fallback to a default model. The claude-cli dropdown is now also fed from the daemon; the old hardcoded web list is dropped from this form (retained only for the legacy `ConnectionScreen`).

## 2026-07-19 — Workflows spawn a child conversation as a session tab

- A managed workflow (first: **grill-me** after its Create-plan phase) can hand off to a fresh implementation agent by spawning a new interactive conversation on its own git worktree; the new conversation appears as a **tab inside the parent session**, beside the Agent tab and any bash tabs. See [session-terminal-tabs.md](session-terminal-tabs.md).
- Child tabs are discovered from the existing session list (no new RPC): any session whose `orchestrator_session_id` points at the open session renders as `sessions-child-tab-<id>`; selecting it attaches and shows that child session's live pane. A session with no children shows only the Agent tab.

## 2026-07-22 — Code pane syntax highlighting

- The Worktree **Code pane** file preview now syntax-highlights recognized code files (Rust, TS/TSX, Python, JSON, YAML, and more) instead of showing plain monospace text. The language is inferred from the file's extension; files with no recognized extension (e.g. `LICENSE`) stay plain, and Markdown keeps its sanitized-markup rendering. Highlight colors follow the app's light/dark theme. See [session-code-pane.md](session-code-pane.md).

## 2026-07-22 — Agent Activity pane

- Every session pane gains a top-right **activity icon** (shown only once the session has recorded at least one agent tool call) that opens an overlay listing **one-line records of the agent's own tool calls** — Read, Shell/Bash, Edit, and `tddy-tools` verbs — with `[running]`/`[error]` markers. Records stream in real time; newly-arrived activity carries an **unread badge** until the overlay is opened.
- Selecting a row opens a scrollable **detail dialog** showing the call's full input and full output (Escape- and backdrop-close). The pane is session-type-agnostic — it renders the same for tool, cursor-cli, claude-cli, and **sandbox** sessions.
- Backed by a new per-session `agent-activity.jsonl` log + shared `AgentActivityRecord` (in `tddy-core`), and two new `ConnectionService` RPCs — server-streaming `StreamSessionActivity` (snapshot-then-live) and unary `ReportAgentActivity` (claude-cli hook → daemon). See [agent-activity-pane.md](agent-activity-pane.md).

## 2026-07-22 — Worktree Code pane

- Every session — terminal, workflow chat, and PR-Stack — gains a **Code** toggle that splits the main pane, opening a directory tree of the session's worktree files beside the live view (with a draggable divider). Toggling it never disturbs the running terminal or chat.
- The tree loads lazily one folder at a time and respects `.gitignore` (and hides `.git`), so build output and secrets like `.env` never appear. Selecting a file shows a read-only preview — Markdown rendered, everything else as monospace text.
- See [session-code-pane.md](session-code-pane.md).

## 2026-07-21 — Host stats footer (disk + per-core CPU; traffic relocated)

- The sessions drawer gains a persistent bottom **Host Stats Footer**. The byte-traffic readout moves out of the top header into this footer; the top header now holds only the daemon selector.
- The footer adds two host-level indicators for the currently selected daemon: **available disk space** on the filesystem holding the daemon's default project directory (refreshed every 60 s), and a row of **per-core CPU** mini bars (refreshed every 5 s). Switching the selected daemon re-fetches both for the new host.
- Backed by two new `ConnectionService` RPCs (`GetHostCpuStats` / `GetHostDiskStats`, sourced from `sysinfo` on the daemon). See [host-stats-footer.md](host-stats-footer.md).

## 2026-07-21 — Reusable Agent Chat, ACP transport, transcript export

- The PR-Stack chat is extracted into a recipe-agnostic `AgentChat` / `useAgentChat` (with `agent-chat-*` test ids and an `AgentChat.stories.tsx` covering empty / streaming / select / multiSelect / error / connecting); any recipe can mount it. See [session-drawer.md § Agent Chat](session-drawer.md#agent-chat).
- `AgentChat` gains an `acp` prop selecting the ACP protobuf mirror (`useAcpSession` over `AcpService.Session`) instead of the default `TddyRemote.Stream`, over the same LiveKit room; both hooks return the identical result and render through a shared `AgentChatView`. **The pr-stack chat now uses ACP**, at full behavior parity (goal/activity bubbles, single/multi-select clarifications, "other" free-text, streaming, error banners) via documented tddy conventions on ACP fields.
- New "Export" button downloads a plain-text transcript with ISO timestamps, merging chat messages and clarification (elicitation) points into one chronological timeline — so the operator can see what the agent did and when, including where it paused for input. Works on both transports.

## 2026-07-16 — Session terminal tabs

- The session detail pane now has a terminal tab bar: an **Agent** tab (the coding agent, not closable) plus one closable tab per interactive **bash** terminal, with a `+` to open more; switching tabs keeps every terminal of the session mounted and streaming in the background.
- Works for local (gRPC) and remote/coder (LiveKit) sessions, reusing the existing `terminal_id`-addressed `ConnectionService` terminal RPCs — no protocol changes. See [session-terminal-tabs.md](session-terminal-tabs.md).

## 2026-07-12 — Fast session change: per-session runtimes, session-participant RPC, live inspector bytes

- Switching between attached LiveKit sessions is now a focus change, not a reconnect: each attached session owns its own LiveKit `Room` + `GhosttyTerminalLiveKit` instance held in a `SessionRuntimeRegistry`; the focused terminal is CSS-visible while the others stay mounted (`display:none`) and keep streaming. No unmount, no terminal resize, no LiveKit reconnect on switch.
- Session-scoped `ConnectionService` RPCs (`ExecuteTool`, `ListExecTools`, `ListSessionToolCalls`, `ClaimTerminalControl`/`WatchTerminalControl`, VNC, screen-sharing) route to the session's own participant (`daemon-{instanceId}-{sessionId}`); `DeleteSession`/`SignalSession` and bootstrap/directory RPCs stay daemon-direct on `daemon-{instanceId}` so lifecycle control still works when the coder participant is stuck.
- The sessions list overlays `session` participant metadata (goal/state/agent/model/…) onto active cross-host rows — presence-driven, no `ListSessions` fan-out for active rows.
- The Session Inspector **Details** tab shows live bytes in / bytes out and a "last data received: Ns ago" relative timestamp; for a session with no LiveKit participant it falls back to daemon `SessionEntry` (`bytes_in`/`bytes_out`/`last_data_received_at`) fields.
- Feature: [session-drawer.md § Fast Session Change](session-drawer.md#fast-session-change), [web-terminal.md § Per-session LiveKit room](web-terminal.md#per-session-livekit-room-sessions-drawer), [livekit-participant-owned-projects.md](livekit-participant-owned-projects.md). PR [#297](https://github.com/uppin/tddy-coder/pull/297).
## 2026-07-12 — Session Inspector: real-time token usage

- The Session Inspector has a new **Usage** tab showing live per-conversation token usage (main agent + each subagent: input/output/total tokens and turns) with a summing TOTAL row, updating as the session runs.
- Usage streams over the session's existing event stream (`TddyRemote.Stream`) — no new endpoint — and a running session now broadcasts updates as its agents produce turns.
- Known limitations: a View opened mid-session sees totals on the next update tick (no snapshot-on-connect yet), and end-to-end live rendering still depends on the per-session presenter stream's LiveKit room wiring. See [session-usage-inspector.md](session-usage-inspector.md). PR [#295](https://github.com/uppin/tddy-coder/pull/295).
## 2026-07-12 — Active sessions across hosts in the sessions drawer

- A session that's live (has a LiveKit participant) now appears in the sessions drawer no matter which host is selected — it's tagged with its owning host and stays fully interactive (connect/resume/terminate route to that host).
- Feature: [session-drawer.md § Cross-Host Active Sessions](session-drawer.md#cross-host-active-sessions). PR [#294](https://github.com/uppin/tddy-coder/pull/294).

## 2026-07-06 — Cursor CLI sandbox + managed workflow in CreateSessionPane

- **Cursor Agent CLI** sessions expose the same **Sandbox** toggle and **Managed codebase** section (recipe picker + specialized-subagents multi-select) as Claude CLI.
- Cypress `CreateSessionCursorCliAcceptance` covers sandbox and managed-codebase request fields. Feature: [cursor-cli-session.md](../daemon/cursor-cli-session.md). PR [#287](https://github.com/uppin/tddy-coder/pull/287).

## 2026-07-04 — Multi-daemon session host selection & auto-provision

- New sessions can pick which host/daemon runs them; the picker appears when more than one daemon is in the common room.
- "Add to host" now reliably reaches the host you chose, and lets you optionally set where the clone lands; each host shows its base clone location.
- Starting a PR-stack session opens the normal create-session form pre-filled from the planned PR, so you can review and adjust before it spawns.
- Starting a session on a host that doesn't have the project yet auto-clones it there (into the host's base location) instead of failing.
- See [projects-screen-multi-host.md](projects-screen-multi-host.md), [daemon-selector-livekit-rpc.md](daemon-selector-livekit-rpc.md), [session-drawer.md](session-drawer.md).

## 2026-07-04 — Daemon selector + LiveKit-only daemon-level RPC

- Every daemon-mode screen's header now shows a daemon selector, listing the daemons currently in the common LiveKit room; defaults to the daemon serving this web session.
- Switching daemons re-targets projects, worktrees, VMs, tasks, and session-list RPC at the selected daemon over LiveKit — no page reload, and no dependency on the target daemon's own HTTP origin.
- Per-session terminal and PR-Stack chat connections are unaffected — they keep talking to their own session's LiveKit room regardless of which daemon is selected.
- See [daemon-selector-livekit-rpc.md](daemon-selector-livekit-rpc.md).

## 2026-07-03 — Dedicated Projects screen + multi-host projects

- New `/projects` screen (nav menu item) lists projects grouped by project, showing every host a project lives on.
- "Add to host" adds an existing project to another connected daemon, reusing its project id; target hosts come from the connected daemon participants.
- Create-project moved from the old sessions view to the new screen; the sessions view's per-project session list is unchanged.
- See [projects-screen-multi-host.md](projects-screen-multi-host.md).

## 2026-07-01 — PR stack parent picker for Claude CLI sessions

- Claude CLI sessions can now be placed in a PR stack by selecting a parent in the new-session form, with git-base chaining automatically applied (child worktree branches off the parent's branch)
- Parent picker now renders for **both Tool and Claude CLI** session types (previously tool-only)
- Picker filters to **PR-stack orchestrator sessions only** (recipe `orchestrate-pr-stack` or `plan-pr-stack`), including childless orchestrators — replaces the old child-derived heuristic

## 2026-06-26 — Screen Sharing tab with VNC and RDP protocol selector

- Session inspector gains a **Screen Sharing** tab (alongside Details, Tools, and VNC) accessible from any session
- Add form has a **protocol selector** (VNC / RDP) that auto-fills the default port (5900 / 3389); selecting RDP reveals a **username field**
- First vault operation (add target with password) prompts for the vault passphrase; subsequent adds in the same session skip the dialog (`vaultUnlocked` session guard)
- **Start** calls `ScreenSharingService.StartStream`; daemon dispatches to the VNC or RDP bridge binary and opens the full-screen LiveKit overlay
- Inline error messages appear below the form on `AddTarget`, `UnlockVault`, or `StartStream` failures (no silent swallowing)
- Username stored on the target and threaded through to the RDP `IronRDP` credential handshake (was hardcoded `"user"`)

## 2026-06-26 — PR-stack session UI: recipe dropdown, parent picker, collapsible drawer groups

- `CreateSessionPane` recipe field replaced with a `<select>` listing all 9 workflow recipes (tdd, tdd-small, bugfix, free-prompting, grill-me, review, merge-pr, plan-pr-stack, orchestrate-pr-stack); default is "tdd"
- New parent-picker `<select>` (tool sessions only): lists orchestrator sessions so a child can be attached to an existing PR-stack; hidden for claude-cli sessions
- `SessionDrawer` groups PR-stack children under the orchestrator session in a collapsible `<details>/<summary>` element; children render at `depth={1}` (indented)
- Orphan children (orchestrator not present in the list) fall through to the flat list
- New utils: `stackParentCandidates(sessions)`, `groupSessionsByStack(sessions)`
## 2026-06-26 — Browser DEBUG mask + fix SendTerminalInput unhandled rejections

- `dev.daemon.yaml` ships `debug: "tddy:term:*"` — a [`debug`](https://www.npmjs.com/package/debug)-package namespace mask served at `GET /api/config`; browser adopts it on load with `localStorage` persistence (invalidated only when the config value changes); `?debug=` URL param overrides for a session
- `GhosttyTerminal` / `GhosttyTerminalGrpc` replace ad-hoc `console.log` spam with namespaced loggers: `tddy:term:{write,data,resize,grpc,life,mouse}`
- Fixed `GrpcSessionTerminal.send()`: unhandled `[failed_precondition]` promise rejections silenced via `.catch(() => {})`; `controlToken` prop added and forwarded in every `SendTerminalInput` call (internal ref pattern — stream is not recreated on token changes)
- `useTerminalControl` exposes `controlTokenRef`; token threads `SessionsDrawerScreen` → `SessionMainPane` → `GrpcSessionTerminal`
## 2026-06-26 — VNC sessions: inspector tab, encrypted vault, full-screen overlay

- Session inspector drawer gains a **VNC** tab alongside Details and Tools; accessible regardless of session connection state
- `SessionVncTab` lists configured VNC targets (label, host:port, per-target status) and provides an Add form (label, host, port, optional password)
- First vault operation (add with password, start stream) triggers `VncPassphraseDialog`; passphrase creates/unlocks the vault (Argon2id + ChaCha20-Poly1305 AEAD, `.vnc.yaml` mode 0600); derived key cached in daemon memory for the session
- Per-target **Start** calls `VncService.StartVncStream`; daemon spawns a `tddy-vnc` bridge binary that publishes a LiveKit video track; per-target **Stop** calls `StopVncStream` and tears down the process
- **VNC overlay**: full-screen (`fixed inset-0 z-50`) darkened overlay renders the remote desktop video; dismiss via Escape, backdrop click, or close button
- Per-target **Remove** calls `VncService.RemoveVncTarget` and deletes the encrypted credential
- New `tddy-vnc` package scaffolded with `common.rs` (`char_to_keysym`, `rgba_to_abgr`); bridge pump loop and VncClient/VncStreamer are follow-up stubs (FIXME)
- Feature: [vnc-sessions.md](vnc-sessions.md)

## 2026-06-26 — PTY terminal width fix — gRPC session terminal renders at correct width

- New `GrpcSessionTerminal` component: measures its container's pixel width/height, computes `initial_cols`/`initial_rows` (8px × 17px character-cell estimates), and passes them to the `StreamTerminalOutput` gRPC request so the daemon resizes the PTY before forwarding output
- `GhosttyTerminalGrpc` gains a hidden `data-testid="terminal-buffer-text"` div (200 ms polling) enabling Cypress to assert visible terminal text without OCR
- New `GrpcSessionTerminalResize.cy.tsx` component tests (3) verify `initial_cols > 0`, `initial_rows > 0`, and that cols match container width
- New `terminal-rendering.cy.ts` e2e tests (4) against a live daemon with `tddy-demo-tui`: AC1 width ≠ 220, AC2 no horizontal overflow, AC3 resize updates cols, AC4 reconnect shows correct width immediately
## 2026-06-26 — Single-screen terminal control mutex: Claim terminal CTA

- `SessionMainPane` gains a `terminalControl` prop; when another screen holds the lease an absolute scrim overlay appears with the holder's screen id and a **"Claim terminal"** button
- `useTerminalControl` hook: claims control (steal=false) on session attach, subscribes to `WatchTerminalControl` server-stream for real-time lease-change events, exposes `claim()` for steal=true
- `terminalControlState.ts` pure reducer folds `TerminalControlEvent` stream into `{ isController, holderScreenId }`
- `screenId.ts`: stable per-browser-tab identity persisted in `sessionStorage` (two tabs get different ids)
- `SessionsDrawerScreen` owns the hook and passes `terminalControl` to `SessionMainPane` only when a session is connected

## 2026-06-25 — Session inspector Tools tab: invoke panel + durable call log

- Session inspector drawer gains a **Details / Tools** tab strip; Details tab is selected by default (existing metadata/controls unchanged)
- **Invoke panel**: tool picker (`ListExecTools`), JSON args textarea seeded from tool's JSON Schema (`defaultArgsFromSchema`), Invoke button calls `ExecuteTool`; result renders in a code block; errors show a styled error box
- **Call log**: collapsible rows newest-first from new `ListSessionToolCalls` RPC; each row shows tool name + status; expanded row shows Input (`args_json`), Output (`result_json`), and stdio panels (Shell rows parse `stdout`/`stderr`/`exit_code`)
- **Durable persistence**: every `ExecuteTool` invocation is appended to `~/.tddy/sessions/{id}/tool-calls.jsonl`; the log survives daemon restarts and in-memory registry eviction
- Log is scoped per session and capped at 500 most-recent entries; empty state message when no calls recorded
- After a successful invoke the call log automatically refetches to show the new row
- Feature: [session-drawer.md](session-drawer.md)

## 2026-06-25 — Tasks UI: real-time two-pane view with WatchTaskList streaming

- `/tasks` upgraded from 3-second polling table to `TasksDrawerScreen`: live two-pane layout (left drawer + right output pane)
- `useTaskListStream` subscribes to new `WatchTaskList` server-streaming RPC; `Map<taskId, TaskInfo>` updated in real time without polling
- `TaskDrawerItem`: status dot (blue/gray/green/red/yellow by status), kind text (truncated), inline Cancel button for pending/running tasks; newest-first order
- `TaskOutputPane`: per-channel tabs (one per `TaskChannelInfo`); `TaskChannelOutput` streams bytes via existing `WatchTask` RPC with auto-scroll
- Feature: [tasks-ui-realtime.md](tasks-ui-realtime.md)
## 2026-06-25 — Create new session from sessions drawer

- `+ New session` button in the `SessionDrawer` header switches `SessionsDrawerScreen` to `"creating"` mode
- `CreateSessionPane` replaces the main pane: tool vs Claude CLI toggle; project (required), agent/recipe or model/permission-mode/initial-prompt fields; branch intent (new branch from base or work on existing branch with `ListProjectBranches` dropdown)
- On submit: `StartSession` RPC; on success: auto-navigate to `/sessions/:newId` and auto-attach via `ConnectSession`; on error: inline error message, form stays open
- Cancel returns to the previous session list / placeholder state
- 29 new Cypress component tests (12 acceptance, 17 unit)
## 2026-06-25 — Terminal mobile shortcut drawer

- `ShortcutDrawer` component: floating drag-to-snap panel (`position: fixed`; snaps to nearest screen edge on drag release; `data-snap-edge` attribute); renders one button per preset; each click sends the correct ANSI/VT byte sequence via `pushInput`
- `toolShortcuts.ts`: `ToolShortcutDef` interface, `TOOL_SHORTCUTS` map (tddy-coder: Shift+Tab/Ctrl+C/Escape; claude-cli: Escape/Ctrl+R/Ctrl+C), `keySequenceToBytes` (named keys, Ctrl+letter, F1–F12, single chars), `toolIdentifierFromPath`, `resolveShortcutsForSession`
- `GhosttyTerminalLiveKit`: new `mobileShortcuts?: ToolShortcutDef[]` and `mobileShortcutsViewportHeight?: number` props; drawer renders when `showMobileKeyboard && mobileShortcuts.length > 0`
- `LiveKitConnectionParams`: `shortcuts?: ToolShortcutDef[]` field; `resolveShortcutsForSession` called at all 5 `addSessionAttachment` sites in `ConnectionScreen`
- Cypress component tests: `ShortcutDrawer.cy.tsx` (render, empty, click-to-send, drag-snap, row layout); `GhosttyTerminalLiveKit.cy.tsx` 3 integration tests; fixed 2 pre-existing flaky Disconnect tests (stub alias shadowing + `{ force: true }` for canvas `mousedown` interception)
- Feature: [web-terminal.md](web-terminal.md)

## 2026-06-21 — Auth redirect: all daemon pages require login

- All daemon-mode pages now gate on auth at the `App` level; unauthenticated visitors see a login screen with "Sign in with GitHub"
- `login(returnTo?)` saves the current hash path to `sessionStorage`; `AuthCallback` redirects to `/#<returnTo>` after OAuth completes, returning users to the page they were trying to access

## 2026-06-21 — Session Inspector Drawer

- `SessionInspectorDrawer` overlay panel: `data-state="closed" | "open" | "expanded"`; header with expand/restore + close buttons; scrollable metadata section (all `SessionEntry` fields, empty omitted); controls (Resume / Delete with two-click confirm / Terminate SIGTERM)
- `inspectorState.ts`: pure `defaultInspectorOpen(isActive)` + `nextInspectorState(state, action)` reducer (actions: open/close/toggle/expand/restore/select)
- `SessionMainPane` (repurposed from `SessionDetailPane`): inspector toggle button, connected-terminal branch, disconnected placeholder; inspector open by default for disconnected sessions
- `useSessionAttachment`: added `deleteSession` (DeleteSession RPC) and `signalSession` (SignalSession RPC) actions
- Proto `SessionEntry` extended with five new fields (tool, sessionType, updatedAt, livekitRoom, previousSessionId) surfaced from `.session.yaml`; `hook_token` never exposed
- Feature: [session-drawer.md](session-drawer.md)

## 2026-06-21 — Session Drawer Screen

- New `#/sessions` route and `SessionsDrawerScreen`: left-side drawer listing all sessions newest-first, detail pane showing terminal (connected) or Resume + metadata (disconnected)
- `SessionDrawerItem`: derived label (`repoPath` basename → `workflowGoal` → `sessionId.slice(0,8)`), status dot (connected / disconnected / needs-input), focus tooltip with full session id
- `useSessionAttachment` hook: single-session `ConnectSession` / `ResumeSession` attach lifecycle, `connected-livekit` and `connected-grpc` states
- New shadcn primitives: `tooltip.tsx`, `scroll-area.tsx`; new utils: `sessionDrawerLabel`, `connectionStatusForSession`, `sortSessionsByCreation`
- Feature: [session-drawer.md](session-drawer.md)

## 2026-06-21 — Demo goal Phase 2: DemoVmControls

- `DemoVmControls` component: polls `GetDemoVmStatus` every 3 s; "Launch Demo VM" → `StartDemoVm`, "Stop VM" → `StopDemoVm`, booting badge, running state with "Open demo" share-URL link, error with "Retry"
- Wired into `ConnectionScreen.tsx` for sessions with `workflowGoal === "demo"` alongside the session token guard
- Feature: [coder/demo-goal.md](../coder/demo-goal.md). Cross-package: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — Codex OAuth: operator tunnel in tddy-daemon

- **Docs**: **[codex-oauth-web-relay.md](codex-oauth-web-relay.md)** — operator callback TCP and **`StreamBytes`** run in **`tddy-daemon`** when using desktop + **`livekit.common_room`**; session-side **`LoopbackTunnelService`** semantics unchanged. **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — Codex OAuth: loopback tunnel documentation

- **Docs**: **[codex-oauth-web-relay.md](codex-oauth-web-relay.md)** documents session-side **`LoopbackTunnelService`** semantics (privileged port refusal, first **`TunnelChunk`**); desktop flow cross-links **[tddy-desktop-electrobun.md](../desktop/tddy-desktop-electrobun.md)**. **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).

## 2026-04-11 — Connection screen: multi-host eligible daemons (LiveKit common room)

- **`tddy-web`**: **ConnectionScreen** sorts **ListEligibleDaemons** for the Host dropdown (**local** first, then **`instance_id`**); **StartSession** sends the selected **`daemonInstanceId`** when the daemon lists multiple eligible hosts. Cypress **ConnectionScreen** covers multi-row host list and multi-session disconnect scoping. **Feature docs**: [web-terminal.md](web-terminal.md), [livekit-peer-discovery.md](../daemon/livekit-peer-discovery.md). **Cross-package**: [docs/dev/changesets.md](../../dev/changesets.md).
## 2026-04-11 — Connection screen: multi-daemon project rows and host-scoped sessions

- **`tddy-web`**: **`ListProjects`** rows carry **`daemon_instance_id`**; **`ConnectionScreen`** renders one accordion and session table per row, with composite **`data-testid`** keys **`projectId__daemonInstanceId`** when the field is set; **`sessionProjectTable`** helpers (**`connectionProjectRowKey`**, **`sessionBelongsToProjectHost`**, **`sortedSessionsForProjectHostTable`**, **`isSessionOrphan`**) scope sessions and unscoped repo matching per host. Cypress **`ConnectionScreen.cy.tsx`** covers multi-host listing and collision cases; Bun **`sessionProjectTableMultiHost.test.ts`** covers table helpers. Feature doc: [web-terminal.md](web-terminal.md).

## 2026-04-11 — LiveKit presence: owned project count

- **tddy-web**: **`ParticipantList`** **Projects** column for **`owned_project_count`** in participant metadata (**`parseOwnedProjectCount`**, **`OWNED_PROJECT_COUNT_METADATA_KEY`**); em dash when the field is absent; **`useRoomParticipants`** supplies **`ownedProjectCount`** from LiveKit metadata; Cypress **`ParticipantList.cy.tsx`** covers render and metadata updates. Feature doc: [livekit-participant-owned-projects.md](livekit-participant-owned-projects.md).

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
