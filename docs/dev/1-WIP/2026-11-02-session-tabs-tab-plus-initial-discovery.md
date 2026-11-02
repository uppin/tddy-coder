# Initial discovery — #session-tabs 5/7 `tab-plus`

**Date:** 2026-11-02
**Changeset:** [2026-11-02-session-tabs-tab-plus.md](./2026-11-02-session-tabs-tab-plus.md)

Explorations 1–3 below are the full whole-work discovery of the `#session-tabs` stack, copied verbatim at planning time; the conclusions relevant to this node are the ones its changeset's State A distils. Node-specific follow-up passes are appended as later `## Exploration N` sections.

## Combined conclusions

1. **Tabs on a dormant session are blocked in two places.**
   - Daemon: `CliManagerTerminalRoster::start`
     (`packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs` L167–171)
     returns `failed_precondition("session has no running terminal")` when no `"main"` PTY is in
     `CliSessionManager`. It needs `main` only to read the worktree path.
   - Web: `SessionMainPane.tsx:178` passes `focused={!dormant && …}`, and `SessionRuntime.tsx:250`
     adds `hidden`, so the whole tab strip — "+", terminals, agent-conversation tabs — disappears
     on a dormant session. The `SessionActivitiesPane` overlay covers the runtime layer.
2. **"Add agent" has no daemon liveness check.** `attach_session_agent` resolves only the session
   dir. The web "Add agent" button (`SessionMainPane.tsx:306`) is gated on `client` alone, but the
   conversation tabs it opens render inside the hidden runtime.
3. **Tabs are in-memory only.** `CliSessionManager` keeps `session_id → terminal_id → TerminalEntry`.
   Nothing is persisted, resume recreates only `main`, and main's exit does not stop other tabs.
4. **"Active" means "the recorded pid is alive"** (`session_reader.rs` L83). The web derives
   `isDormant` from `!session.isActive` (`connectionStatusForSession.ts`, `sessionBaseView.ts`).
5. **Resume:** `SessionService.ResumeSession` → `resume_session_at_session_coordinate`
   (`tddy-agent-launch/src/svc_resume_session.rs`). The web calls it from `handleResume`
   (`useSessionsDrawerPaneActions.ts:83-102`). The top-bar Resume button is at
   `SessionMainPane.tsx:291-302`.
6. **Greyed history exists for live terminals only.** `GhosttyTerminalSession` has `HISTORY_THEME` /
   `HISTORY_FILTER` (`grayscale(1)`) and a page pane fed by `GetTerminalHistory`. A dormant session
   shows only the ACP transcript (`SessionActivitiesPane` → `AgentTranscriptView`, read-only).
7. **No toast component exists.** The only toast-like UI is `TerminalControlOverlay`. Nothing is
   installed from npm for toasts.
8. **"+" is outside the scroller,** at the right edge (`SessionTerminalTabs.tsx:118` `flex-1
   overflow-x-auto` list, then `NewTerminalMenu` at :207, then fullscreen). The doc comment at :84-86
   records why: so overflow cannot push it out of reach.
9. **No tmux anywhere** in Rust, TS, protos or scripts.
10. **No OSC 52 / terminal copy handling.** The web has `copyToClipboard` (`lib/clipboard.ts`), used
    only by the Files tab. The terminal does paste only. There is no Tauri clipboard plugin.
11. **Every served terminal is a daemon-spawned PTY** (or jail PTY). Tabs go through
    `TerminalSessionService.StartTerminalSession` with a `kind` (bash or a CLI harness). The coder
    participant roster runs bash only. Started tabs are never jailed: `os_user: None`, so they run
    in main's worktree as the daemon user.
12. **Sandboxed sessions** keep their PTY in `SandboxSessionManager`. `StartTerminalSession` on one
    hits the same `failed_precondition`, because the CLI manager holds no `main` for it. This is
    inferred from code; no test covers it.

## Exploration 1 — web (tddy-web) session tabs, inactive view, history, toasts, clipboard

No `.codegraph/`, so grep plus reads. Paths are under `packages/tddy-web/` unless stated.

### 1. Tab bar

**Files:** `src/components/sessions/SessionTerminalTabs.tsx`, `NewTerminalMenu.tsx`, `useSessionTerminals.ts`, `cliHarnessOptions.ts`, `agentConversationTabs.ts`, `SessionRuntime.tsx`, `SessionMainPane.tsx`.
**Grep:** `tablist|role="tab"|add.?tab|new.?shell|NewTerminalMenu`, `add agent|attachSessionAgent`.

**Component:** `SessionTerminalTabs`, rendered once per runtime at `SessionRuntime.tsx:253`. The tab order is:
- a fixed, non-closable **Agent** tab (`data-testid="sessions-terminal-tab-agent"`, :119-127);
- one tab per terminal, each with a close ✕ (:129-153), labelled "Bash 1", "Warp 1" and so on (`labelledTerminals`, :61-70);
- child-session tabs (`sessions-child-tab-${id}`, :155-170);
- agent-conversation tabs with a close ✕ (`sessions-agent-tab-${id}`, :175-204).

**Where "+" sits:** outside the scrolling tab list.
- The tab list is `<div className="flex min-w-0 flex-1 items-center overflow-x-auto">` (:118) and closes at :205.
- `<NewTerminalMenu …/>` comes next (:207), then the optional full-screen toggle (:210-226).
- Because the list is `flex-1`, "+" sits at the right edge with ⛶ to its right.
- Doc comment :84-86: "the "+" and the full-screen control sit *outside* that scroller, so a session with a dozen terminals and conversations cannot push them out of reach".

**What "+" offers:** `NewTerminalMenu.tsx:49-52`:
```ts
const menuKinds = [
  BASH_KIND,
  ...(sessionType === TOOL_SESSION_TYPE ? [] : CLI_HARNESS_OPTIONS.map(({ id }) => id)),
];
```
- Button: `data-testid="sessions-terminal-tab-new"`, `aria-label="Open a new terminal"` (:56-67).
- Menu: `sessions-terminal-tab-new-menu`. Items: `sessions-terminal-tab-new-${kind}` (:69-90).
- Closes on Escape or an outside click (:28-46).
- The harness list is in `cliHarnessOptions.ts:4-11`: Claude CLI, Cursor CLI, OpenCode, Muse Code, Warp. `tool` is filtered out (:20).
- "+" does **not** offer "add agent".

**"Add agent" is a separate top-bar button:** `SessionMainPane.tsx:306-317`, `data-testid="session-agent-attach-btn"`, shown only when `client` exists. It opens `AgentPicker` (:341-350). The attach call is `sessionAgentClient.attachSessionAgent` (`useSessionAgentConversations.ts:66`).

**Data model:**
- `useSessionTerminals.ts:13` `AGENT_TERMINAL_ID = "main"`; :16-19 `interface SessionTerminal { readonly terminalId: string; readonly kind: string; }`.
- The list is local React state (:68), filled from `client.listTerminalSessions` (:78) and merged with tabs opened locally.
- `open` calls `startTerminalSession` (:120); bash is sent as an empty `kind` (:117). `close` removes the tab first, then calls `stopTerminalSession` (:156-163).
- Agent conversations: `AgentConversation {conversationId, agentId, label, daemonInstanceId}` (`agentConversationTabs.ts:12-28`). Held as React state in `useSessionAgentConversations`, keyed by session (`SessionMainPane.tsx:91-99, 170-171`).
- Child sessions come from `useChildSessions(runtime.sessionId, sessions)` (`SessionRuntime.tsx:135`).
- Service: `TerminalSessionService` (`packages/tddy-terminal-rpc/proto/terminal_session.proto:26-28`).

### 2. Inactive sessions

**Files:** `sessionBaseView.ts`, `src/utils/connectionStatusForSession.ts`, `SessionMainPane.tsx`, `SessionInspectorDrawer.tsx`, `useSessionsDrawerPaneActions.ts`, `useSessionAttachment.ts`, `packages/tddy-service/proto/session.proto`.
**Grep:** `onResume|resumeSession|handleResume`.

**What counts as dormant:**
- `connectionStatusForSession.ts:21-23`: `if (!session.isActive) { return "disconnected"; }`.
- `sessionBaseView.ts:17-19`: `isDormant` is true when the status is `"disconnected"`.
- `sessionBaseView.ts:28-34`: `sessionBaseViewMode` returns `"workflow"` if the session has a workflow view, otherwise `"activities"` when dormant and `"terminal"` when live.
- `sessionBaseView.ts:38`: `canResumeSession` equals `isDormant`.

**What the view shows:** the overlay for a dormant session is `SessionActivitiesPane`, the recorded transcript (`SessionMainPane.tsx:199-208`). The runtime layer stays mounted but gets `hidden` (:224-228). With no runtimes, no overlay and no connection, the pane shows "Select Resume to reconnect" (:240-244).

**Resume:**
- Top bar, `SessionMainPane.tsx:291-302`: `{dormant && (<Button data-testid={`sessions-main-resume-${selectedSession.sessionId}`} … onClick={() => onResume(selectedSession.sessionId)}>Resume`.
- Inspector, `SessionInspectorDrawer.tsx:336-346`: shown when `!session.isActive`.
- `SessionDetailPane.tsx:91` has another `onResume` button.
- `handleResume` (`useSessionsDrawerPaneActions.ts:83-102`) → `resumeSession(sessionId, sessionToken, activeHost)` → `useSessionAttachment.ts:134-137` `client.resumeSession({ sessionToken, sessionId })`.
- RPC: `SessionService.ResumeSession` (`session.proto:38`). Request `{session_token, session_id}` (:213-216). Response `{session_id, livekit_room, livekit_url, livekit_server_identity}` (:218-223).

**How the actions are gated:**
- `SessionMainPane.tsx:178` passes `focused={!dormant && r.sessionId === focusedRuntimeId}`.
- `SessionRuntime.tsx:250` applies `className={cn("absolute inset-0 flex h-full w-full flex-col", focused ? "" : "hidden")}`.
- So the whole strip, "+" included, is **hidden**, not disabled. The activities overlay also hides the runtime layer.
- Neither `NewTerminalMenu` nor `SessionTerminalTabs` has a disabled or inactive prop.
- "Add agent" is gated only on `client` (`SessionMainPane.tsx:306`).
- Inspector Delete is gated on `!session.isActive && !pendingDelete` (`SessionInspectorDrawer.tsx:348`).

### 3. History views

**Files:** `docs/dev/1-WIP/2026-10-09-terminal-manual-history-toggle.md`, `src/components/GhosttyTerminalSession.tsx`, `src/rpc/connections/terminal.ts`, `src/lib/terminalHistoryLoader.ts`, `SessionActivitiesPane.tsx`, `chat/AgentChat.tsx`, `chat/AgentTranscriptView.tsx`, `chat/useAcpReplay.ts`.
**Grep:** `manual.?history|scrollback|snapshot|HISTORY_THEME|View history|Back to live`.

**(a) Greyed terminal history pane** (`GhosttyTerminalSession`):
- `:91` `const HISTORY_THEME = { background: "#232326", foreground: "#9a9aa0" };`
- `:92` `const HISTORY_FILTER = "grayscale(1) brightness(0.9)";`
- `:647-649`: `historyOffered = canScrollBack && !liveTuiOwnsNavigation`, plus `loadEarlierVisible` and `viewHistoryVisible`.
- Buttons: "Load earlier output" (`load-earlier-history`, :851-860), "View history" (`view-history`, :862-870), "Back to live" (`back-to-live`, :906-914).
- Page pane: `data-testid="terminal-page-pane"`, `data-color-scheme="history"`, `filter: HISTORY_FILTER` (:892-904).
- Data: `feed.history` (`terminal.ts:86-88`, `feedSupportsHistory` :174-178), from `TerminalSessionService.GetTerminalHistory` via `createForwardHistoryFetcher` (`terminalHistoryLoader.ts:40-48`).

**(b) Read-only transcript for dormant sessions:** `SessionActivitiesPane` renders `AgentChatView` with `readOnly` (`SessionActivitiesPane.tsx:95`). `AgentChat.tsx:32-35, 90-92` switches to `AgentTranscriptView`. Data: `ActivityService.streamAcpReplay` / `getAcpReplayPage` (`useAcpReplay.ts:123, 163, 219`).

**Not found:** a terminal-snapshot view for a dormant session.

### 4. Toasts

**Grep:** `toast|sonner|notistack|Snackbar` across the four web packages and `package.json`.
- **Not found:** a toast library or a generic toast component.
- `TerminalControlOverlay` (`src/components/sessions/TerminalControlOverlay.tsx`) is the "Claim terminal" toast: `data-testid="terminal-control-overlay"`, `className="pointer-events-auto absolute left-1/2 top-3 z-10 …"`. Used at `SessionRuntime.tsx:437-444`; the z-order is explained at :429-431.
- Inline errors use `InlineError` (`src/components/ui/inline-error.tsx:18`, `role="alert"`).
- `sessionNotificationRegistry.ts` drives the drawer's indicator dots; it is not a toast.

### 5. Terminal rendering

**Files:** `src/components/GhosttyTerminal.tsx`, `GhosttyTerminalSession.tsx`, `sessions/GrpcSessionTerminal.tsx`, `sessions/useSessionTerminalFeed.ts`, `rpc/connections/{localHost.ts,hostServedSession.ts,livekit/sessionConnection.ts}`, `lib/clipboard.ts`.
- `GhosttyTerminal.tsx:9` imports `{ Terminal, FitAddon } from "ghostty-web"` (`^0.4.0`, `package.json:36`). `new Terminal({` at :313, `term.onData` at :332, `term.open(...)` at :373, `paste(text)` at :1322.
- Agent tab over a media connection: `useSessionTerminalFeed` → `connection.openTerminal(...)` (`useSessionTerminalFeed.ts:78`) → `<GhosttyTerminalSession feed=…>` (`SessionRuntime.tsx:296-309`). The LiveKit implementation is `livekit/sessionConnection.ts:255-265` (`StreamTerminalIO` plus host `GetTerminalHistory`).
- Agent tab without media, and every bash or harness tab: `GrpcSessionTerminal` (`SessionRuntime.tsx:317-328, 333-350`), which calls `client.streamSessionTerminalIO(` (`GrpcSessionTerminal.tsx:294`) and gets history via `createForwardHistoryFetcher` (:403).
- Clipboard: `lib/clipboard.ts:11-23` `copyToClipboard` (`navigator.clipboard.writeText`, with an `execCommand("copy")` fallback at :26-40). Its only user is `SessionFilesTab.tsx:146`.
- **Not found:** OSC 52 (`OSC ?52|osc52|\]52;`), and any terminal copy-selection handler. The Ctrl/Meta keydown handler (`GhosttyTerminal.tsx:586-620`) handles zoom only.

### 6. tmux

`grep -rli tmux` over the web packages and all `packages/**/*.{rs,proto,ts,tsx}`: **not found**.

### 7. Tests, stories and helpers

- `mountWithRpc`: `cypress/support/rpc/inMemory.tsx:39`. `anInMemoryRpcBackend`: `packages/tddy-connectrpc-testkit/src/backend.ts:276`, re-exported from `src/test-utils/builders.ts:14`.
- Backends: `cypress/support/rpc/terminalSessionServiceBackend.ts`, `daemonSessionHostBackend.ts`, `sessionServiceBackend.ts`, `agentConversationBackend.ts`, `acpReplay.ts`.
- Page objects: `cypress/support/pages/sessionTerminalTabsPage.ts`, `sessionsDrawerPage.ts`, `sessionActivitiesPage.ts`. Drivers: `cypress/support/drivers/terminalHistoryPagingDriver.tsx`, `ghosttyTerminalSessionDriver.tsx`.
- Tab-bar specs: `SessionTerminalTabsAcceptance.cy.tsx`, `SessionTerminalTabsAgentKindsAcceptance.cy.tsx` (:85 '+' menu, :158 tool session, :173 Escape), `SessionTerminalTabsAgentTabs.cy.tsx`, `SessionChildTabsAcceptance.cy.tsx`, `SessionAgentAttachTabAcceptance.cy.tsx`, `SessionTerminalFullscreenAcceptance.cy.tsx`.
- Inactive and resume specs: `InactiveSessionActivitiesAcceptance.cy.tsx` (:233, :248, :308), `SessionInactiveInspectorOverlay.cy.tsx`, `SessionStartResumeStillDaemonRouted.cy.tsx`, `WorkflowChatResumeAcceptance.cy.tsx`.
- Terminal and history specs: `TerminalHistoryPaging.cy.tsx` (:185), `GrpcSessionTerminal*.cy.tsx`, `GhosttyTerminal*.cy.tsx`.
- No test asserts the position of "+". No stories exist for the tab bar.
- Feature docs: `docs/ft/web/session-terminal-tabs.md`, `inactive-session-activities.md`, `terminal-replay-lazy-scroll.md`, `web-terminal.md`.

## Exploration 2 — daemon terminals, liveness, resume, LiveKit, tmux, clipboard, sandbox

Pattern set: `StartTerminalSession|start_terminal|failed_precondition|is_active|ResumeSession|tmux|clipboard|pbcopy|osc.?52`.

### 1. Server-side tab creation

- Proto `packages/tddy-terminal-rpc/proto/terminal_session.proto`:
  - `TerminalSessionService` L10–34; Start/Stop/List at L26–28. Main is the reserved id `"main"`.
  - `StartTerminalSessionRequest` L195–205: `session_token`, `session_id`, `control_token`, `kind`. `kind` is empty or `"bash"`, or a harness type (`claude-cli`, `cursor-cli`, `opencode-cli`, `muse-cli`, `warp-cli`).
  - `StartTerminalSessionResponse.terminal_id` L207–210. `TerminalSessionInfo` L230–236 has `terminal_id`, `kind`, `pid`.
  - I/O messages carry `terminal_id`; empty means main (L40–41, L93–95).
- Handler `packages/tddy-terminal-rpc/src/service.rs`:
  - `start_terminal_session` L295–309 builds `TerminalLaunch` (kind plus `login_shell_for(&os_user)`) and calls `self.ports.roster.start`.
  - TODO L291–294: `control_token` is never checked.
  - `stop_terminal_session` L311–337 refuses `"main"`.
  - Ports `service/ports.rs`: `TerminalKind::{Bash, Agent(String)}` L71–88, `TerminalRoster` trait L106–117.
- Daemon roster `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs`, `CliManagerTerminalRoster::start` L166–203:
  - Bash: `manager.start_terminal(session_id, worktree, &launch.login_shell)`.
  - Agent: `cli_harness(kind)`, a fresh conversation id, then `start_agent_terminal`.
  - Mounted in `packages/tddy-daemon/src/runtime/mount_entries.rs` L15–17 and in session rooms via `session_room_roster()` (`svc_session_files_ports.rs` ~L76–90).
- Coder roster `packages/tddy-coder/src/session_participant/terminal_session_service.rs` L233–299: bash only; its `list` omits main.
- PTY layer `packages/tddy-cli-sessions/src/cli_session_manager/`:
  - `cli_session_manager.rs` L22 `MAIN_TERMINAL_ID = "main"`. The index (L48–56) is an in-memory `Arc<RwLock<HashMap>>`.
  - `terminals.rs`: `start_terminal` L24–44 (uuid v7, `os_user: None`), `start_agent_terminal` L49–70, `list_terminals` L83, `stop_terminal` L103, `stop_session` L128.
  - `pty_spawn.rs`: `spawn_tool` L41–112 → `PtyRuntime::spawn`. Cleanup L161–214 drops an entry when its process exits. Main's exit drops the workflow and LiveKit exposure (L209–212) but not the other tabs.
- `packages/tddy-pty/src/lib.rs` is transport-agnostic. `packages/tddy-terminal-rpc/src/pty_runtime.rs` resolves `os_user`/setpriv (L117–145).
- Persistence: `SessionMetadata` (`packages/tddy-changeset/src/session_metadata.rs` L12–128) has no tabs field. It persists `agents: Vec<SessionAgentRecord>` and `agents_rev` (L68–77).
- Added agents: `packages/tddy-service/proto/session_agents.proto` `SessionAgentService` L23–83. `AttachSessionAgent(agent_id = "name@daemon_instance_id", placement)`. Handler `packages/tddy-session-agents/src/service.rs` L631ff.

### 2. Liveness requirements

- Tabs, `svc_terminal_ports.rs` L167–171:
  ```rust
  let main = self.manager.get(session_id).await
      .ok_or_else(|| Status::failed_precondition("session has no running terminal"))?;
  let worktree = main.worktree_path.clone();
  ```
  The check exists only to read the worktree (doc comment L164–165). The string appears nowhere else.
- The coder side refuses a foreign session id (`terminal_session_service.rs` L278–282). Streaming a missing terminal returns `not_found` (`service.rs` L122, L203).
- Roster agents: **no check**. `roster_session_dir` (`packages/tddy-session-agents/src/svc_resolve_listed_worktree.rs` L150–164) authenticates, maps the user and calls `session_dir_for`. The proto comment still says "a LIVE session" (L24–25).
- "Active" means `metadata.pid.map(is_pid_alive)` (`packages/tddy-session-activity/src/session_reader.rs` L83, `kill(pid,0)` L43–46). Proto field `SessionEntry.is_active` (session.proto L277); there is also a free-form `status` (L274).

### 3. Resume

- `SessionService.ResumeSession` (session.proto L38) → `svc_session_lifecycle_ports.rs` L70–77 → `packages/tddy-agent-launch/src/svc_resume_session.rs` `resume_session_at_session_coordinate` L33–221. It branches by `session_type`: claude-cli, cursor-cli, other harnesses (sandboxed or plain), workspace (re-provisions the jail), and tool by default (respawns tddy-coder).
- claude-cli `svc_resume_claude_cli_session.rs` L21–161: `manager.resume_with_options` (`relaunch.rs` L44–69) recreates main and rewrites `.session.yaml` with `status: active` and the new pid. No LiveKit room is returned; the terminal is bridged when a consumer arrives.
- Only main is restored; the extra tabs are not.

### 4. Terminal bytes over LiveKit

- `packages/tddy-livekit`: protobuf RPC over reliable data channels (`client.rs` L287–295, `participant/publisher.rs` L38–92, `chunking.rs`). No media tracks are used for terminals.
- `packages/tddy-daemon-livekit/src/terminal_exposure.rs` `LiveKitTerminalExposure::expose` (L50ff).
- `cli_session_manager/exposed_terminals.rs` `ensure_exposed_terminal` (L70–122): lazy, needs the live main PTY, wraps it with `pty_terminal_service(handle)` → `terminal.TerminalService/StreamTerminalIO`.
- `packages/tddy-session-lifecycle/src/terminal_session_adapter.rs` `DaemonTerminalSessionStore` (L174–213): looks in the sandbox registry first, then `CliSessionManager`.
- Every served terminal is a daemon-spawned PTY or a jail PTY.

### 5. tmux

`grep -rniI tmux packages scripts`: nothing. Repo-wide, only `docs/future/tui-e2e-testing.md` L29 (pytest-tmux).

### 6. Clipboard

`clipboard|pbcopy|osc.?52|]52;`:
- Rust: only `packages/tddy-vnc/src/vnc_client.rs` L237 (a comment).
- Web: `lib/clipboard.ts`. The terminal does paste only (`GhosttyTerminal.tsx` L184–186, L1322).
- Desktop: no Tauri clipboard plugin.

### 7. Sandbox

- A jailed PTY lives in `SandboxSessionManager` (`packages/tddy-daemon-sandbox/src/sandbox_session.rs` L31, L125).
- `terminal_session_adapter.rs` L187–203: a sandboxed session has only main; `StartTerminalSession` never reaches a jail.
- Started tabs are never jailed (`os_user: None`); `login_shell` is looked up for the mapped OS user.
- `packages/tddy-sandbox-runner/src/harness_pty.rs` hosts in-jail PTYs for the main harness only.

## Exploration 3 — follow-up reads for the node plans (planner, direct reads)

Files read: `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs` (whole),
`packages/tddy-terminal-rpc/src/service/ports.rs` L60–130, `packages/tddy-terminal-rpc/src/service.rs`
L285–340, `packages/tddy-terminal-rpc/src/bridge.rs` L750–800, `packages/tddy-task/src/terminal_capture.rs`
L1–60 + `pub fn` list, `packages/tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs` L30–115,
`packages/tddy-changeset/src/session_metadata.rs` L12–47, `packages/tddy-agent-launch/src/svc_resume_claude_cli_session.rs`
L30–75, `packages/tddy-web/src/components/sessions/SessionMainPane.tsx` L150–250,
`packages/tddy-web/src/components/sessions/SessionTerminalTabs.tsx` L80–120, `packages/tddy-web/package.json`.
Greps: `os_user` in `cli_session_manager/*.rs`; `sessions_base_for_user`; `get_terminal_history`;
`terminal_session|.proto"` in `packages/tddy-service/tests/unbundle_service_split.rs`.

- **The roster never sees the OS user.** `start_terminal_session` resolves `os_user` from the token, uses it
  only for `login_shell_for(&os_user)`, and passes `TerminalLaunch { kind, login_shell }` — no user. A
  metadata lookup (n1) and a run-as-user spawn (n3) both need it on the launch.
- **`TerminalKind::from_request`**: `"" | "bash"` → `Bash`, anything else → `Agent(kind)`. A `tmux` kind must
  be matched before the `Agent` arm.
- **Session dirs:** `tddy_daemon_kernel::user_paths::sessions_base_for_user(os_user, config_dir)` (L51);
  `SessionMetadata.repo_path: Option<String>` is the worktree. Resume derives the worktree as split
  context dir → `repo_path` → session dir (`svc_resume_claude_cli_session.rs` L58–62).
- **Spawn as a user:** `spawn_tool(…, os_user: Option<&str>, on_exit)` → `PtySpawnSpec.os_user`. Started
  tabs pass `None` (`terminals.rs` L40); the main launch passes the session's user (`launch.rs` L44, L184).
- **History is RAM only:** `serve_get_terminal_history_with` does `store.get_terminal(...)` →
  `not_found("terminal not found or not running")`, then `session.capture().lock().replay_from(from, until, max)`.
  `TerminalCapture` (712 production lines, over budget) holds `buffer`, `start_offset`, `end_offset`,
  modes and an `avt::Vt`; public: `start_offset()`, `end_offset()`, `buffered_bytes()`, `replay_from`,
  `replay_before`, `replay_last`. Nothing writes it to disk.
- **Dormant web rendering:** `SessionMainPane` builds `runtimeLayer` from `runtimes` (attached sessions
  only) and an `overlay` (`SessionActivitiesPane` when `baseViewMode === "activities"`); the overlay hides
  the layer. A dormant session with no runtime shows the overlay alone. So a dormant tab strip needs a
  runtime-less host component, not just un-hiding `SessionRuntime`.
- **"+" menu clipping:** `SessionTerminalTabs` doc comment: "+" sits outside the scroller "and the menu is
  not clipped by it" — moving "+" into the `overflow-x-auto` list clips an absolutely-positioned menu.
- **Web unit tests** run under `bun test` (`test:unit` covers `src/lib`, `src/components/sessions`, …).
- **Proto tripwire:** `unbundle_service_split.rs` covers `connection`/`host`/`worktree`/`livekit`/`session*`
  protos, **not** `terminal_session.proto` — a new terminal RPC does not trip it.
- **ghostty-web OSC 52 support:** not verifiable here (`node_modules` absent in this worktree) — pass 2 checks.
