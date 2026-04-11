# Web Terminal

## Summary

A web application that displays an interactive terminal in the browser using the ghostty-web terminal emulator. **tddy-demo TUI streaming over LiveKit** is implemented: GhosttyTerminal component in tddy-web receives ANSI bytes via TerminalService RPC, with Cypress E2E validation. A standalone generic terminal (user's default shell over WebSocket) remains available via the Ghostty-web demo. In **daemon mode**, **ConnectionScreen** treats **`ListProjects`** as a row registry (**`ProjectEntry.daemon_instance_id`** identifies the owning daemon): one accordion and session table per row, with session assignment scoped to each session’s host.

## Recommended Setup

Use the official Ghostty-web demo:

```bash
./serve_term.sh
# or: npx @ghostty-web/demo@next
```

Serves at http://localhost:8080 with WebSocket PTY at `/ws`. Works best on Linux and macOS.

## Background

tddy-coder currently operates as a CLI/TUI application. To enable remote observation and collaboration, a web-based terminal viewer is needed. A previously implemented Dioxus fullstack solution was archived; its architecture is documented in [docs/kb/dioxus-web-terminal.md](../../kb/dioxus-web-terminal.md).

## Architecture (Reference)

- **Terminal emulator**: ghostty-web npm package (WASM-compiled Ghostty VT100 parser, xterm.js-compatible API)
- **PTY**: Server-side PTY spawning the user's default `$SHELL`
- **Communication**: WebSocket between browser (ghostty-web Terminal) and server (PTY process)

## Connected Terminal UX

When the terminal connects and renders, it supports:

- **Fullscreen**: Fills 100% of the viewport (width and height). **Connection chrome** overlays sit above the terminal canvas (high `z-index`, pointer events on controls only).
- **Auto-focus**: Keyboard focus is set on the terminal when ready. User can type immediately. (On mobile, auto-focus is disabled; see Mobile UX.)
- **Adaptive size**: FitAddon auto-sizes the terminal to its container. Resize events are sent to the virtual TUI via `\x1b]resize;{cols};{rows}\x07`.
- **Font zoom (pitch)**: The terminal supports **pitch-in** (larger glyphs), **pitch-out** (smaller glyphs), and **reset** to the session baseline. There are **no on-screen +/−/0 buttons**; zoom is via **keyboard** (**Ctrl** or **⌘** with **+**/**=**, **-**, or **0** when focus is inside **`[data-testid='ghostty-terminal']`**), **two-finger touch pinch**, **trackpad pinch** (**`wheel`** with **`ctrlKey`**), or programmatic **`CustomEvent`** dispatch. Default font bounds are **8–32** px with step **1**; at the minimum or maximum, further pitch-in or pitch-out is ignored. **`GhosttyTerminal`** exposes the live size on **`data-terminal-font-size`** (integer string). Font changes apply to the running ghostty-web **`Terminal`** (`options.fontSize`), then **`FitAddon.fit()`** recomputes columns and rows; **`onResize`** runs when the grid changes, so the existing resize OSC sequence reaches the TUI backend on the same input path as keyboard data. **`GhosttyTerminalLiveKit`** accepts **`fontSize`** (default **14**) and passes it to **`GhosttyTerminal`** as the reset baseline. Bridge events use **`tddy-terminal-zoom`** and **`tddy-terminal-font-size-sync`**; payloads are validated before handling. Optional trace logging uses **`VITE_TERMINAL_ZOOM_DEBUG=true`** in the Vite build, or **`debugLogging`** on **`GhosttyTerminal`**. Implementation reference: [terminal-zoom.md](../../../packages/tddy-web/docs/terminal-zoom.md).
- **Touch/mouse mode**: When `--mouse` is set on tddy-coder, the TUI sends EnableMouseCapture. GhosttyTerminal encodes SGR mouse sequences `\x1b[<Pb;Px;PyM/m` (press/release) and forwards them via onData. Click-to-select and scroll work. Touch events (touchstart/touchend) are forwarded for tap-to-click on mobile. The TUI draws Enter and (when wide enough) Stop affordances to the right of the prompt; see [Mouse mode: Enter control](../coder/tui-status-bar.md#mouse-mode-enter-control) and [Mouse mode: Stop control](../coder/tui-status-bar.md#mouse-mode-stop-control).

### Connection chrome (LiveKit overlay)

When **`GhosttyTerminalLiveKit`** is mounted with **`connectionOverlay`**, connection controls render in a dedicated top row (**`TerminalConnectionStatusBar`**, `data-testid="terminal-connection-status-bar"`) above the Ghostty terminal area. The row uses **`role="toolbar"`** and **`aria-label="Terminal connection"`**. **`ConnectionTerminalChrome`** supplies the interactive content; supported layouts are **`corner`** (controls over the terminal canvas), **`paneHeader`** (compact dot + menu for floating toolbars), and **`statusBar`** (horizontal toolbar: build id, status dot, fullscreen; no overlay on the grid). **`GhosttyTerminalLiveKit`**, **`ConnectionScreen`**, and the standalone connected view use **`chromeLayout="statusBar"`** inside the status bar wrapper.

**Placement modes** (**`connectionChromePlacement`** on **`GhosttyTerminalLiveKit`**, default **`floating`**):

- **`floating`**: Full status bar — build id, status dot, fullscreen control, and an optional trailing slot for the mobile keyboard affordance when **`showMobileKeyboard`** applies.
- **`none`**: Compact status bar — status dot and menu (and optional mobile keyboard slot); build id and fullscreen controls are omitted so overlay / mini terminal presentations stay unobstructed.

The shell includes:

- **Build ID**: Shown in the status bar row when provided (`data-testid="build-id"`) whenever the placement mode includes it.
- **Status dot**: **`data-testid="connection-status-dot"`**. Attribute **`data-connection-status`** reads **`connecting`**, **`connected`**, or **`error`** for the LiveKit / token phase. While **`connecting`**, the dot uses a pulse animation; steady colors distinguish **`connected`** and **`error`**. Users who prefer reduced motion receive a non-animated connecting state via **`prefers-reduced-motion`**.
- **LiveKit status strip**: With the overlay enabled, the plain **`livekit-status`** row does not occupy layout during **`connecting`** or **`connected`**; the dot carries those phases. Token, room, and stream failures surface through **`data-testid="livekit-error"`** and related error UI.
- **Fullscreen**: A dedicated control (`data-testid="terminal-fullscreen-button"`, in the status bar row when placement is **`floating`**) enters or exits document fullscreen on the connected terminal subtree. The implementation uses the standard Fullscreen API with vendor-prefixed fallbacks (`packages/tddy-web/src/lib/browserFullscreen.ts`). The parent passes **`fullscreenTargetRef`** to select the element; when absent, chrome supplies an internal fullscreen target wrapper (`data-testid="connection-chrome-fullscreen-fallback-target"`). **`fullscreenchange`** and **`webkitfullscreenchange`** on **`document`** keep the control label in sync with the active element.
- **Menu**: Activating the dot opens a menu with **Disconnect** (`data-testid="connection-menu-disconnect"`) and **Terminate** (`data-testid="connection-menu-terminate"`) when the host passes **`onTerminate`** (daemon flows with session context). The standalone GitHub connect flow omits **Terminate** when no session-backed handler exists. **Terminate** runs a native **`window.confirm`** dialog; **`onTerminate`** runs only after the user confirms. The menu closes on **Escape** or an outside pointer press.
- **Interrupt (Stop)**: There is no web **Stop** button; interrupt is the TUI **Stop** pane (red **U+25A0**), to the right of the Enter strip. Clicks are SGR mouse bytes to the virtual TUI, same path as keyboard **Ctrl+C** (byte **0x03**).

**Layout acceptance (tests)**: Pure geometry helpers in **`terminalStatusBarLayout.ts`** express rules such as “status bar bottom meets or above terminal top” and “control centers lie outside the terminal canvas.” Bun tests cover **`terminalStatusBarLayout`**; Cypress **`GhosttyTerminalLiveKit.cy.tsx`** imports the same helpers so assertions stay aligned with the library.

**ConnectedTerminal** wrappers (**App** after connect and **ConnectionScreen** after session connect) render the fullscreen **`connected-terminal-container`** with this chrome during JWT acquisition so the status dot reflects the loading phase while **`livekit-status`** text stays suppressed for normal overlay states. On the daemon **ConnectionScreen**, each attached session can mount under a per-session root with **`data-testid="connection-attached-terminal-{sessionId}"`** (URL-encoded segment in the attribute matches the session id string).

Implementation reference: [terminal-connection-chrome.md](../../../packages/tddy-web/docs/terminal-connection-chrome.md).

### Mobile UX

On touch-capable devices or narrow viewports (width &lt; 768px):

- **Keyboard-aware resize**: The terminal container uses the Visual Viewport API. When the virtual keyboard opens, the container shrinks to fit the visible area above the keyboard; when it closes, the terminal fills the screen again.
- **Manual keyboard button**: A floating "Keyboard" button appears at the bottom center. Tapping it focuses the terminal (opens the virtual keyboard). The button hides while the keyboard is open and reappears when it closes.
- **Focus prevention**: Tapping the terminal does not open the keyboard. The terminal uses `preventFocusOnTap` (event prevention + readonly textarea) so the keyboard opens only when the user taps the Keyboard button.
- **Touch forwarding**: Tap-to-click works for TUI menus and interactive elements. Capture-phase touch handlers send SGR mouse sequences before focus prevention, so interactive TUIs (vim, htop) receive correct mouse events. A **second finger** on the surface does not emit SGR press/release pairs (avoids confusing the TUI during pinch). **Two-finger pinch** on the terminal adjusts **font size** (same bounds and steps as pitch in/out); disable with **`pinchZoomFont={false}`** on **`GhosttyTerminal`** if needed.
- **Build ID**: A build timestamp is shown in the top-left when connected for cache verification on mobile.

## Daemon mode: Connection screen (project-centric)

When `tddy-daemon` serves the web bundle (`daemon_mode: true`), authenticated users see **ConnectionScreen** (not the manual LiveKit URL form):

### URL routes (daemon mode)

- **Session list**: `/` — project tables, **Other sessions**, create project, presence panel when configured.
- **Terminal**: `/terminal/{sessionId}` — one URL-encoded path segment after the fixed prefix `/terminal`. **`terminalPathForSessionId`** and **`terminalDeepLinkSessionPath`** build the same encoded path for navigation and deep links.
- **Project detail**: `/project/{rowKey}` — one URL-encoded path segment after the fixed prefix `/project`. The segment carries the stable registry key from **`connectionProjectRowKey`** (**`projectId`** or **`projectId__daemonInstanceId`**). **`projectPathForRowKey`** and **`parseProjectRowKeyFromPathname`** in **`packages/tddy-web/src/routing/appRoutes.ts`** build and parse this path. **`parseProjectRowKeyForConnectionScreen`** in **`packages/tddy-web/src/routing/projectRoute.ts`** supplies **`ConnectionScreen`** with the decoded key. Multi-segment paths under **`/project/`** and bare **`/project`** yield no decoded key (same single-segment rule as **`/terminal/`**). Invalid percent-sequences yield no decoded key.

**Concurrent terminal attachments**

- The client keeps a **map** of **`sessionId` → LiveKit connection parameters** for every active attachment. There is **no** fixed cap on how many sessions attach at once; practical limits follow browser memory and WebRTC resources.
- **Start New Session**, **Connect**, and successful deep-link **connectSession** / **resumeSession** calls **merge** a new entry or **replace** the params for an existing **`sessionId`** in that map. A second **Connect** on a different session **does not** remove the first; each session keeps its own LiveKit room name, identities, and debug flag.
- **Focused session**: The path `/terminal/{sessionId}` selects the focused attachment when that id is present in the map. With multiple attachments, the address bar path determines which session owns **fullscreen** presentation; **overlay** / **mini** render **one** floating **`ConnectedTerminal`** per attached session (stacked under **`terminal-reconnect-overlay-root`**), each wrapped for automation with **`data-testid="connection-attached-terminal-{sessionId}"`**.
- **Disconnect** on one session removes **only** that map entry and reconciles the URL to a remaining attachment when needed. **popstate** navigation to the session list (`/`) clears **all** attachments and sets presentation to **`hidden`**. **popstate** to a **`/project/...`** URL does not clear the attachment map by itself; **popstate** to **`/`** clears **all** attachments.
- **`ListSessions`** polling: when a row for an attached session becomes inactive, the client drops **that** attachment from the map; other sessions stay attached.

**Attach behavior and presentation**

- **`nextPresentationFromAttach`** classifies **new** attach (**Start**, **Connect**, successful deep-link **connectSession**) vs **reconnect** attach (**Resume**, successful deep-link **resumeSession**). Both kinds select **`overlay`** presentation from the helper without an automatic history **push**; the screen applies a **`replace`** navigation to `/terminal/{sessionId}` after a successful attach so the address bar matches the focused session. The session list remains visible behind floating terminals unless the user expands to **fullscreen**.
- **Expand** on a floating preview **pushes** the terminal route and switches to **`full`** presentation. **Back** on the dedicated fullscreen terminal switches to **`mini`** presentation without tearing down LiveKit for that session and without issuing another **connectSession** / **resumeSession**.

**Deep link** (`/terminal/{id}` on full page load): **`ConnectionScreen`** attaches with **`connectSession`**, then **`resumeSession`** if connect fails. If the session is **already** in the attachment map, the deep-link effect does not issue another RPC.

- **Disconnect** (from the dot menu on a given terminal) removes that session from the map and updates history when other attachments remain. **popstate** to `/` clears **all** attachments and resets presentation to **`hidden`**. A full page load on `/terminal/{id}` loads the SPA (same **`index.html`** as `/`; the static server uses SPA fallback for unknown paths). If the id is not in **`ListSessions`**, a **session not found** banner appears with **Back to sessions** (returns to `/`).
- **OAuth**: `/auth/callback` is unchanged; **`App`** renders **`AuthCallback`** for that path.

Implementation reference: **`terminalPresentation`** helpers ([terminal-presentation.md](../../../packages/tddy-web/docs/terminal-presentation.md)), **`ConnectionScreen`**, **`appRoutes.ts`**.

**Standalone** (`daemon_mode: false`): connection uses **query parameters** (`url`, `identity`, `roomName`, optional `debug`). A **`/terminal/...`** path is not part of the standalone flow; the client **replaces** the URL with **`/`** on load if such a path is present so the address bar matches the documented standalone model.

Implementation helpers: **`packages/tddy-web/src/routing/appRoutes.ts`**. Static serving: **`packages/tddy-coder/src/web_server.rs`** (`ServeDir` fallback to **`index.html`**).

- **Create project** (collapsible): name + git URL → `CreateProject` (clone or adopt existing path under `~/repos/<name>/` by default). Optional **path under home** overrides the clone destination (e.g. `Code/my-app`).
- **Projects** as collapsible sections (`<details>`): each row comes from **`ListProjects`** and represents one registry entry, including **`daemon_instance_id`** (the owning daemon for that row). The session list screen renders **one accordion per row**: when **`daemon_instance_id`** is non-empty, automation keys use **`projectId__daemonInstanceId`** (for example **`data-testid="project-accordion-{projectId}__{daemonInstanceId}"`** and **`data-testid="sessions-table-{projectId}__{daemonInstanceId}"`**); when it is empty (legacy single-daemon payloads), keys use **`projectId`** alone. The same logical **`project_id`** on two daemons therefore appears as **two** accordions, each with its own session table and **Start New Session** form state. Each section shows name, git URL, **`main_repo_path`**, a visible host label derived from the row’s **`daemon_instance_id`**, then **Host** (target daemon instance from **`ListEligibleDaemons`** for the next **`StartSession`**), **Tool** (options from **`ListTools`**, reflecting daemon **`allowed_tools`**), **Backend** (options from **`ListAgents`**, reflecting daemon **`allowed_agents`**; each option’s value is the agent **`id`** sent on **`StartSession.agent`**; the selected backend is the first list entry unless a prior choice for that composite row still appears in the list), **Workflow recipe** (control defaults to **`free-prompting`**; **`StartSession.recipe`** accepts **`tdd`**, **`tdd-small`**, **`bugfix`**, **`free-prompting`**, or **`grill-me`**, aligned with **`recipe_resolve`**), and **Debug logging** (browser terminal only)—all **per session**, not stored on the project—then **Start New Session** (**`StartSession`** with **`project_id`**, optional **`daemon_instance_id`**, and **`recipe`**), and a table of sessions that belong to **that** project row (**matching `project_id` and session `daemon_instance_id`**). Session tables include a **Host** column (**`daemon_instance_id`** from **`ListSessions`**). Connect/Resume in that section uses that row’s debug setting.
- **Home session preview (per project row on `/`)**: The accordion table lists **at most ten** sessions in the same order as the full list (**`sortedSessionsForProjectHostTable`** then **`splitSortedSessionsForHomePreview`** in **`packages/tddy-web/src/lib/projectSessionsHomePreview.ts`**; limit **`HOME_PROJECT_SESSIONS_PREVIEW_LIMIT`**). When more than ten sessions exist, a summary (**`data-testid="project-sessions-overflow-summary-{rowKey}"`**) and a **View all sessions for this project** control (**`data-testid="project-sessions-overflow-nav-{rowKey}"`**, button with descriptive text) navigate to **`/project/{encodedRowKey}`**. On **`/`**, the header **select all** checkbox and bulk **Delete selected** apply to **visible** preview rows only.
- **Project detail screen (`/project/{encodedRowKey}`)**: When the decoded key matches a **`ListProjects`** row, **`ConnectionScreen`** renders **`data-testid="project-screen-root"`**, the full session table (**`project-screen-sessions-table-{rowKey}`**) with the same row actions as the home table (connect/resume, signals, delete, bulk selection, workflow columns, **More** menu), plus **`ProjectSessionOptions`** and **Start New Session** (**`project-screen-start-session-{rowKey}`**) using the same **`handleStartSession`** path as the accordion. When the key matches no row after **`ListSessions`** / **`ListProjects`** hydration, the UI shows **Project not found** (**`project-route-not-found`**) and **Back to sessions** (**`project-route-not-found-home`**) returning to **`/`**.
- After authentication, the client loads **Tool** and **Backend** options together (`ListTools` and `ListAgents`); a failure in either RPC clears both lists and surfaces an error in the shared connection error area.
- **Other sessions**: Connect/Resume uses a separate **debug** checkbox for that list (sessions not tied to a listed project).
- Sessions whose **`project_id`** is not registered on **that session’s host** (among listed project rows) appear under **Other sessions**. Scoped rows match **`project_id`** on a project whose **`daemon_instance_id`** equals the session’s **`daemon_instance_id`**.
- **Project association for unscoped sessions**: When **`project_id`** is empty, resolution runs only against **project rows whose `daemon_instance_id` matches the session’s `daemon_instance_id`**. Among those, the UI assigns a session to a project if **`repoPath`** equals that project’s **`mainRepoPath`** or is a subdirectory of it (git worktrees under the main clone). If several projects on **that host** could match, the **longest** **`mainRepoPath`** wins.

### Session table ordering

Each project’s session table and the **Other sessions** table list rows in this order:

1. **Active sessions** (`isActive` true) appear before inactive rows.
2. Within the active group and within the inactive group, rows follow **`createdAt`** descending (newer timestamps first), using ISO-8601 strings parsed with the browser **`Date`** implementation.
3. When two rows share the same comparable time, or when **`createdAt`** does not parse to a finite time, order follows **`sessionId`** lexicographically (deterministic tie-break).

The client applies **`sortSessionsForDisplay`** (`packages/tddy-web/src/utils/sessionSort.ts`) to the session array already held in React state after **`ListSessions`**—no additional RPC for ordering.

### Session workflow status (TUI parity)

Project session tables and the **Other sessions** table include five additional columns—**Goal**, **Workflow**, **Elapsed**, **Agent**, and **Model**—alongside ID, Date, Status, Repo, PID, and Actions. The UI renders the string fields on each **`SessionEntry`** returned by **`ListSessions`**: **`workflow_goal`**, **`workflow_state`**, **`elapsed_display`**, **`agent`**, and **`model`**. Empty or whitespace-only values display an em dash (`—`).

### Pending elicitation on session rows

Each **`SessionEntry`** carries **`pending_elicitation`** (RPC / generated client: **`pendingElicitation`**). When **`true`**, the Connection screen shows an **Input needed** badge beside the first-column session id on that row (project tables and **Other sessions**). Each **`<tr>`** exposes **`data-pending-elicitation="true"`** or **`"false"`** for tests and automation; the badge carries an accessible name (**`aria-label`**: session needs input or approval). The value comes from **`pending_elicitation`** in the session directory’s **`.session.yaml`** (**`SessionMetadata`** in **tddy-core**); the running tool is responsible for persisting that flag whenever the workflow blocks on the operator.

The daemon fills the workflow goal, workflow state, elapsed, agent, and model columns from each session directory’s **`.session.yaml`** (session identity) and, when present, **`changeset.yaml`**: the workflow goal is the matching session row’s **tag**; workflow state is **`state.current`**; the agent is the row’s **agent**; the model label is **`models[tag]`** when defined. **Elapsed** is a compact duration string produced with the same rules as the TUI status bar formatter (**`tddy_core::format_elapsed_compact`**), computed from persisted **`state.history`** timestamps (last transition whose state matches **`state.current`**, or **`state.updated_at`**). The browser shows a horizontally scrollable table when the viewport is narrower than the full column set.

While the session list includes at least one row with **`isActive`**, the client requests **`ListSessions`** every **2** seconds; when every row is inactive, the interval is **5** seconds. **`ListProjects`** continues to refresh every **5** seconds. Authentication and user mapping for **`ListSessions`** match other RPCs (GitHub token → mapped OS user → sessions base).

#### TUI vs web elapsed (QA)

- **TUI (`format_status_bar`)**: Elapsed is **`goal_start_time.elapsed()`** — an in-memory **`Instant`** from when the current workflow step started in the running **`tddy-coder`** process.
- **Web / daemon (`ListSessions` enrichment)**: Elapsed is **`format_elapsed_compact(now - step_start)`** where **`step_start`** is parsed from **`changeset.yaml`**: the **`at`** timestamp of the **last** **`state.history`** entry whose **`state`** matches **`state.current`**, or else **`state.updated_at`**. The web shows **persisted** wall-clock duration since the last recorded transition, not the in-process **`Instant`**.
- **Comparison**: When the workflow has **persisted** the latest state to **`changeset.yaml`**, web and TUI **should align** on goal, state, agent, model, and a **similar** elapsed string (same formatting rules in **`tddy_core::format_elapsed_compact`** and TUI **`format_elapsed`**). If the live process has **not yet written** **`changeset.yaml`**, the web may show an **older** elapsed or placeholders until the next **`ListSessions`** poll picks up new disk state.

### Session workflow files (read-only RPCs and preview components)

- **`ListSessionWorkflowFiles`**: Authenticated callers receive **`WorkflowFileEntry`** rows whose **`basename`** values identify allowlisted files present under the resolved session directory (`changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md`). The daemon resolves **`session_id`** server-side; clients do not send filesystem paths.
- **`ReadSessionWorkflowFile`**: Returns **`content_utf8`** for one allowlisted basename under that directory. Traversal-like **`basename`** values and symlink escapes are rejected or omitted per **`session_workflow_files`** rules in **tddy-daemon**.
- **Web** (`packages/tddy-web/src/components/session/`): **`workflowPreviewKind`** classifies filenames for YAML vs Markdown vs plain preview. **`SessionFilesPanel`** lists files and previews content (Markdown as structured line blocks without raw HTML injection; YAML in a monospace **`pre`**). **`SessionMoreActionsMenu`** includes **Show files**, which opens **`SessionWorkflowFilesModal`** (list on open, read on selection). **Cypress** covers **`SessionWorkflowFiles.cy.tsx`**; **Bun** tests cover **`workflowPreviewKind`**. **`ConnectionScreen`** wires the menu and modal on project and **Other sessions** tables.

### Session deletion

- **Delete** (trash): Available for **active** and **inactive** rows. Confirm explains that a running tool process is stopped first, then on-disk data is removed. On success, **`ListSessions`** is refreshed; errors use the shared connection error area.
- **Inactive rows** also show **Resume**; **active** rows show **Connect** and **Signal** (dropdown) alongside **Delete**.
- **Orphan** table follows the same actions pattern as project session tables.

#### Per-table selection and bulk delete

- Each project session table and the **Other sessions** table keeps its own selection: a set of **`sessionId`** values independent of other tables.
- Row checkboxes and a header **select all** control use stable **`data-testid`** values scoped by table: **`session-table-select-all-{tableKey}`** where **`tableKey`** is the same composite key as the accordion (**`projectId__daemonInstanceId`** when **`daemon_instance_id`** is set on the project row, otherwise **`projectId`**), or **`orphan`** for **Other sessions**; row **`session-row-select-{sessionId}`**. The header checkbox is **checked** when every row in that table is selected, **indeterminate** when at least one but not all rows are selected, and **unchecked** when none are selected (including empty tables: unchecked, not indeterminate). The **ID** cell follows the checkbox column: it shows the short session id and, when **`ListSessions`** marks **`pendingElicitation`**, an **Input needed** badge (`data-testid="elicitation-indicator-{sessionId}"`); the row carries **`data-pending-elicitation="true"`** or **`"false"`**.
- **Delete selected** sits in the table toolbar, disabled when the selection is empty. Choosing it opens a single **`window.confirm`** whose text includes the number of sessions to delete and the same stop-then-delete explanation as single-row delete.
- The client sends one **`DeleteSession`** request per selected id in order, awaits each response, then calls **`ListSessions`** and clears that table’s selection when every **`DeleteSession`** succeeds.
- If a **`DeleteSession`** call fails after earlier calls in the same bulk operation have succeeded, the shared connection error area shows the error, **`ListSessions`** runs again, and the table’s selection retains only ids that appear in the refreshed list.
- Dismissing the confirmation dialog does not invoke **`DeleteSession`**.

Pure selection helpers live in **`packages/tddy-web/src/utils/sessionSelection.ts`** (Bun **`sessionSelection.test.ts`**). **`ConnectionScreen`** bulk-path logging is limited to Vite development builds (**`import.meta.env.DEV`**); the **`sessionSelection`** helpers do not emit **`console`** calls in production bundles.

The daemon **`DeleteSession`** uses the same GitHub user → OS user → **`sessions_base`** resolution as **`ListSessions`**, terminates a live **`metadata.pid`** when needed, then removes **`{sessions_base}/sessions/{session_id}/`**. See [daemon changelog](../daemon/changelog.md) and [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md).

See [daemon project concept](../daemon/project-concept.md).

### Shared LiveKit room (`livekit.common_room`)

When the daemon sets **`livekit.common_room`** in YAML, that name is exposed to the web client as **`common_room`** in **`GET /api/config`** (with **`livekit_url`**). After GitHub sign-in, the browser joins that room with identity **`web-{githubLogin}`** and shows a **Connected participants** table on the session list screen (identity, role, joined time, **projects** — integer from participant metadata **`owned_project_count`**, or **—** when absent — **metadata**, Codex sign-in affordance, video column when applicable), refreshed from LiveKit participant events. **Connect** and **Resume** attach the coder terminal in **overlay** / **mini** first; **Expand** switches to **fullscreen** for the focused session — the presence connection stays active in the background while any terminal presentation is open.

If **`common_room`** is unset or blank, that panel is not shown and no extra LiveKit connection is made for presence.

Product reference for the count field and merge semantics: [livekit-participant-owned-projects.md](livekit-participant-owned-projects.md).

Spawned **`tddy-*`** sessions use the same configured room for **`--livekit-room`** when **`common_room`** is set; each process still uses a distinct **`daemon-{session_id}`** LiveKit identity for terminal RPC. If **`common_room`** is unset, the room name is **`daemon-{session_id}`** per session. See [daemon changelog](../daemon/changelog.md).

### Fullscreen terminal session chrome

The fullscreen **GhosttyTerminalLiveKit** view opened after **Expand** from a floating terminal or when the focused session is in **`full`** presentation uses the **connection chrome** described under [Connection chrome (LiveKit overlay)](#connection-chrome-livekit-overlay). **Terminate** in the dot menu, after confirmation, calls **`SignalSession`** with SIGTERM for **that** session’s id (same semantics as **Terminate (SIGTERM)** in the per-session **Signal** dropdown).

### Eligible daemons and host selection

- **`ListEligibleDaemons`**: After sign-in, **ConnectionScreen** loads eligible daemon entries (`instance_id`, `label`, `is_local`) alongside tools and projects. With **`livekit.common_room`** and LiveKit credentials configured on the daemon, the list includes the local daemon plus peers in the same room; otherwise only the local daemon appears.
- **`ListProjects`**: Each **`ProjectEntry`** includes **`daemon_instance_id`** for the owning daemon of that registry row. The daemon builds the local list from disk and may concatenate peer-sourced rows (each tagged with that peer’s **`daemon_instance_id`**) when common-room discovery is enabled; see [LiveKit peer discovery (daemon)](../daemon/livekit-peer-discovery.md).
- **Host dropdown**: Per project row, the selected host is sent as **`daemon_instance_id`** on **`StartSession`**. Empty or matching the local instance selects the local spawn path. A peer **`instance_id`** from the list routes **StartSession** to that daemon over the common-room RPC bridge. Rows are displayed with the local daemon first, then peers ordered by **`instance_id`**.
- **Session host column**: **`ListSessions`** returns **`daemon_instance_id`** per row; the UI shows it in project and **Other sessions** tables.

See [LiveKit peer discovery (daemon)](../daemon/livekit-peer-discovery.md) for configuration, trust model, and RPC semantics.

### Worktrees manager scaffolding

The **Worktrees** product area includes a **`WorktreesScreen`** table component (mocked data in component tests) and a **`tddy-daemon`** **`worktrees`** library for **`git worktree list`**, on-disk stats cache, and **`git worktree remove`**. **ConnectionService** does not expose worktree RPCs yet; shell navigation from the main app to a dedicated route is follow-up work. **`WorktreesAppPage`** does not yet align project identity with composite **`project_id` + `daemon_instance_id`** rows from **`ListProjects`**. Full operator semantics, cache layout, and test commands: [worktrees.md](worktrees.md).

## See also (development)

- [LiveKit and gRPC terminal RPC E2E](../../dev/guides/livekit-terminal-rpc-e2e.md) — `tddy-e2e` tests, VirtualTui vs LiveKit bidi behavior, assertion patterns.

## Future Scope

- **Per-terminal zoom scoping**: with multiple embedded terminals, font zoom bridge listeners should remain scoped per session (see package reference **terminal-zoom.md**).
- Authentication and access control
- Session persistence and reconnection
